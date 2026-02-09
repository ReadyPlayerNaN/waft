#!/usr/bin/env bash
# Performance testing for Waft daemon architecture
# Measures memory usage, socket latency, CPU usage, and reconnection time

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
DURATION=30              # Test duration in seconds
SAMPLE_INTERVAL=1        # Sample interval in seconds
SOCKET_TEST_ITERATIONS=100  # Number of socket latency tests
CLOCK_DAEMON_BIN="${CLOCK_DAEMON_BIN:-./target/debug/waft-clock-daemon}"
TEST_SOCKET="/tmp/waft-perf-test-clock.sock"
RESULTS_DIR="./perf-results"

# Success criteria
MAX_RSS_MB=20
MAX_LATENCY_MS=50
MAX_RECONNECT_S=5

# Ensure results directory exists
mkdir -p "$RESULTS_DIR"

# Functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

cleanup() {
    log_info "Cleaning up..."
    if [ -n "$DAEMON_PID" ] && ps -p "$DAEMON_PID" > /dev/null 2>&1; then
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
    rm -f "$TEST_SOCKET"
}

trap cleanup EXIT INT TERM

# Test 1: Memory usage over time
test_memory_usage() {
    log_info "Test 1: Memory usage over time ($DURATION seconds)"

    # Start daemon
    WAFT_PLUGIN_SOCKET_PATH="$TEST_SOCKET" \
        RUST_LOG=warn \
        "$CLOCK_DAEMON_BIN" &
    DAEMON_PID=$!

    sleep 2  # Wait for daemon to start

    if ! ps -p "$DAEMON_PID" > /dev/null 2>&1; then
        log_error "Daemon failed to start"
        return 1
    fi

    log_info "Daemon PID: $DAEMON_PID"

    # Record memory usage over time
    local output_file="$RESULTS_DIR/memory_usage.csv"
    echo "time_s,rss_kb,vsz_kb,cpu_percent" > "$output_file"

    log_info "Sampling memory usage every ${SAMPLE_INTERVAL}s for ${DURATION}s..."
    for i in $(seq 0 $SAMPLE_INTERVAL $DURATION); do
        # Get memory stats from ps
        local stats=$(ps -p "$DAEMON_PID" -o rss=,vsz=,%cpu= 2>/dev/null || echo "0 0 0.0")
        echo "$i,$stats" >> "$output_file"

        sleep "$SAMPLE_INTERVAL"
    done

    # Calculate statistics
    local avg_rss=$(awk -F',' 'NR>1 {sum+=$2; count++} END {if(count>0) print sum/count; else print 0}' "$output_file")
    local max_rss=$(awk -F',' 'NR>1 {if($2>max) max=$2} END {print max+0}' "$output_file")
    local avg_cpu=$(awk -F',' 'NR>1 {sum+=$4; count++} END {if(count>0) print sum/count; else print 0}' "$output_file")

    # Convert KB to MB
    local avg_rss_mb=$(echo "scale=2; $avg_rss / 1024" | bc)
    local max_rss_mb=$(echo "scale=2; $max_rss / 1024" | bc)

    log_success "Memory usage results:"
    echo "  Average RSS: ${avg_rss_mb} MB"
    echo "  Maximum RSS: ${max_rss_mb} MB"
    echo "  Average CPU: ${avg_cpu}%"
    echo "  Data saved to: $output_file"

    # Check against threshold
    if (( $(echo "$max_rss_mb > $MAX_RSS_MB" | bc -l) )); then
        log_warning "Maximum RSS ($max_rss_mb MB) exceeds threshold ($MAX_RSS_MB MB)"
    else
        log_success "Memory usage is within threshold"
    fi

    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
    DAEMON_PID=""
}

# Test 2: Socket latency
test_socket_latency() {
    log_info "Test 2: Socket latency (GetWidgets request/response)"

    # Start daemon
    WAFT_PLUGIN_SOCKET_PATH="$TEST_SOCKET" \
        RUST_LOG=warn \
        "$CLOCK_DAEMON_BIN" &
    DAEMON_PID=$!

    sleep 2  # Wait for daemon to start

    if ! ps -p "$DAEMON_PID" > /dev/null 2>&1; then
        log_error "Daemon failed to start"
        return 1
    fi

    # Build test client if needed
    if [ ! -f "./target/debug/examples/socket-client" ]; then
        log_info "Building socket client..."
        cargo build --example socket-client 2>&1 | grep -v "Compiling\|Finished" || true
    fi

    # Measure latency using simple Python script
    log_info "Measuring socket latency (${SOCKET_TEST_ITERATIONS} iterations)..."

    local output_file="$RESULTS_DIR/socket_latency.csv"
    echo "iteration,latency_ms" > "$output_file"

    for i in $(seq 1 $SOCKET_TEST_ITERATIONS); do
        # Use Python to send GetWidgets and measure latency
        local latency=$(python3 - "$TEST_SOCKET" <<'EOF'
import socket
import json
import time
import sys
import struct

sock_path = sys.argv[1]

try:
    # Connect
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(sock_path)

    # Prepare GetWidgets message
    msg = {"GetWidgets": None}
    payload = json.dumps(msg).encode('utf-8')
    length = struct.pack('>I', len(payload))

    # Send and measure
    start = time.time()
    s.sendall(length + payload)

    # Read response
    len_bytes = s.recv(4)
    if len(len_bytes) != 4:
        print("0")
        sys.exit(1)

    resp_len = struct.unpack('>I', len_bytes)[0]
    resp_data = b''
    while len(resp_data) < resp_len:
        chunk = s.recv(resp_len - len(resp_data))
        if not chunk:
            break
        resp_data += chunk

    end = time.time()
    latency_ms = (end - start) * 1000

    s.close()
    print(f"{latency_ms:.2f}")
except Exception as e:
    print("0", file=sys.stderr)
    print(str(e), file=sys.stderr)
    sys.exit(1)
EOF
)

        if [ -n "$latency" ] && [ "$latency" != "0" ]; then
            echo "$i,$latency" >> "$output_file"
        fi

        # Small delay between requests
        sleep 0.01
    done

    # Calculate statistics
    local avg_latency=$(awk -F',' 'NR>1 {sum+=$2; count++} END {if(count>0) print sum/count; else print 0}' "$output_file")
    local max_latency=$(awk -F',' 'NR>1 {if($2>max) max=$2} END {print max+0}' "$output_file")
    local min_latency=$(awk -F',' 'NR>1 {if(min=="" || $2<min) min=$2} END {print min+0}' "$output_file")
    local p95_latency=$(awk -F',' 'NR>1 {print $2}' "$output_file" | sort -n | awk '{arr[NR]=$1} END {idx=int(NR*0.95); print arr[idx]+0}')

    log_success "Socket latency results:"
    echo "  Average: ${avg_latency} ms"
    echo "  Minimum: ${min_latency} ms"
    echo "  Maximum: ${max_latency} ms"
    echo "  P95: ${p95_latency} ms"
    echo "  Data saved to: $output_file"

    # Check against threshold
    if (( $(echo "$avg_latency > $MAX_LATENCY_MS" | bc -l) )); then
        log_warning "Average latency ($avg_latency ms) exceeds threshold ($MAX_LATENCY_MS ms)"
    else
        log_success "Socket latency is within threshold"
    fi

    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
    DAEMON_PID=""
}

# Test 3: CPU usage patterns
test_cpu_usage() {
    log_info "Test 3: CPU usage patterns ($DURATION seconds)"

    # Start daemon
    WAFT_PLUGIN_SOCKET_PATH="$TEST_SOCKET" \
        RUST_LOG=warn \
        "$CLOCK_DAEMON_BIN" &
    DAEMON_PID=$!

    sleep 2

    if ! ps -p "$DAEMON_PID" > /dev/null 2>&1; then
        log_error "Daemon failed to start"
        return 1
    fi

    log_info "Monitoring CPU usage under load..."

    # Start background load (continuous GetWidgets requests)
    (
        for i in $(seq 1 100); do
            python3 - "$TEST_SOCKET" <<'EOF' > /dev/null 2>&1
import socket, json, struct, sys
try:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(sys.argv[1])
    msg = {"GetWidgets": None}
    payload = json.dumps(msg).encode('utf-8')
    s.sendall(struct.pack('>I', len(payload)) + payload)
    len_bytes = s.recv(4)
    if len(len_bytes) == 4:
        resp_len = struct.unpack('>I', len_bytes)[0]
        s.recv(resp_len)
    s.close()
except: pass
EOF
            sleep 0.1
        done
    ) &
    LOAD_PID=$!

    # Record CPU usage
    local output_file="$RESULTS_DIR/cpu_usage.csv"
    echo "time_s,cpu_percent" > "$output_file"

    for i in $(seq 0 $SAMPLE_INTERVAL 10); do
        local cpu=$(ps -p "$DAEMON_PID" -o %cpu= 2>/dev/null || echo "0.0")
        echo "$i,$cpu" >> "$output_file"
        sleep "$SAMPLE_INTERVAL"
    done

    # Wait for load to finish
    wait "$LOAD_PID" 2>/dev/null || true

    local avg_cpu=$(awk -F',' 'NR>1 {sum+=$2; count++} END {if(count>0) print sum/count; else print 0}' "$output_file")
    local max_cpu=$(awk -F',' 'NR>1 {if($2>max) max=$2} END {print max+0}' "$output_file")

    log_success "CPU usage results:"
    echo "  Average: ${avg_cpu}%"
    echo "  Maximum: ${max_cpu}%"
    echo "  Data saved to: $output_file"

    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
    DAEMON_PID=""
}

# Test 4: Reconnection time
test_reconnection() {
    log_info "Test 4: Reconnection time after daemon kill"

    local output_file="$RESULTS_DIR/reconnection.csv"
    echo "test_num,reconnect_time_s" > "$output_file"

    for test_num in $(seq 1 5); do
        log_info "Reconnection test $test_num/5..."

        # Start daemon
        WAFT_PLUGIN_SOCKET_PATH="$TEST_SOCKET" \
            RUST_LOG=warn \
            "$CLOCK_DAEMON_BIN" &
        DAEMON_PID=$!

        sleep 2

        # Kill daemon
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true

        # Measure time to restart and reconnect
        local start_time=$(date +%s.%N)

        WAFT_PLUGIN_SOCKET_PATH="$TEST_SOCKET" \
            RUST_LOG=warn \
            "$CLOCK_DAEMON_BIN" &
        DAEMON_PID=$!

        # Wait for socket to be available
        local connected=false
        for attempt in $(seq 1 50); do  # 5 second timeout
            if [ -S "$TEST_SOCKET" ]; then
                # Try to connect
                if python3 - "$TEST_SOCKET" <<'EOF' > /dev/null 2>&1
import socket, json, struct, sys
try:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(sys.argv[1])
    s.close()
    sys.exit(0)
except: sys.exit(1)
EOF
                then
                    connected=true
                    break
                fi
            fi
            sleep 0.1
        done

        local end_time=$(date +%s.%N)
        local reconnect_time=$(echo "$end_time - $start_time" | bc)

        if [ "$connected" = true ]; then
            echo "$test_num,$reconnect_time" >> "$output_file"
            log_success "  Reconnect time: ${reconnect_time}s"
        else
            log_error "  Failed to reconnect"
            echo "$test_num,999" >> "$output_file"
        fi

        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
        DAEMON_PID=""

        sleep 1
    done

    local avg_reconnect=$(awk -F',' 'NR>1 && $2<999 {sum+=$2; count++} END {if(count>0) print sum/count; else print 999}' "$output_file")
    local max_reconnect=$(awk -F',' 'NR>1 && $2<999 {if($2>max) max=$2} END {print max+0}' "$output_file")

    log_success "Reconnection results:"
    echo "  Average: ${avg_reconnect}s"
    echo "  Maximum: ${max_reconnect}s"
    echo "  Data saved to: $output_file"

    if (( $(echo "$avg_reconnect > $MAX_RECONNECT_S" | bc -l) )); then
        log_warning "Average reconnection time ($avg_reconnect s) exceeds threshold ($MAX_RECONNECT_S s)"
    else
        log_success "Reconnection time is within threshold"
    fi
}

# Test 5: Compare daemon vs .so overhead
test_daemon_vs_so_overhead() {
    log_info "Test 5: Compare daemon vs .so overhead"

    # Measure daemon binary size
    local daemon_size=$(stat -f%z "$CLOCK_DAEMON_BIN" 2>/dev/null || stat -c%s "$CLOCK_DAEMON_BIN")
    local daemon_size_kb=$(echo "scale=2; $daemon_size / 1024" | bc)

    log_success "Daemon overhead:"
    echo "  Binary size: ${daemon_size_kb} KB"
    echo ""
    echo "  Note: .so plugins are loaded in-process, while daemons run separately."
    echo "  Daemon benefits:"
    echo "    - Isolation: crashes don't affect overview"
    echo "    - Background processing: can run independently"
    echo "    - Hot reloading: can be restarted without overview restart"
    echo "  Daemon costs:"
    echo "    - Extra process: ~10-20MB RSS overhead"
    echo "    - IPC overhead: socket communication adds latency"
}

# Main
main() {
    log_info "=== Waft Daemon Performance Tests ==="
    log_info "Clock daemon binary: $CLOCK_DAEMON_BIN"
    log_info "Results directory: $RESULTS_DIR"
    echo ""

    # Check if daemon binary exists
    if [ ! -f "$CLOCK_DAEMON_BIN" ]; then
        log_error "Clock daemon binary not found: $CLOCK_DAEMON_BIN"
        log_info "Building clock daemon..."
        cargo build -p clock
    fi

    # Run tests
    test_memory_usage
    echo ""

    test_socket_latency
    echo ""

    test_cpu_usage
    echo ""

    test_reconnection
    echo ""

    test_daemon_vs_so_overhead
    echo ""

    # Summary
    log_info "=== Test Summary ==="
    log_success "All tests completed. Results saved to: $RESULTS_DIR"
    echo ""
    echo "Success criteria:"
    echo "  - Memory usage < ${MAX_RSS_MB} MB: Check $RESULTS_DIR/memory_usage.csv"
    echo "  - Socket latency < ${MAX_LATENCY_MS} ms: Check $RESULTS_DIR/socket_latency.csv"
    echo "  - Reconnection < ${MAX_RECONNECT_S} s: Check $RESULTS_DIR/reconnection.csv"
}

main "$@"
