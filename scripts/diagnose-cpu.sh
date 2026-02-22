#!/usr/bin/env bash
#
# diagnose-cpu.sh — Attach to a running waft daemon process and collect
# evidence to pinpoint 100% CPU usage.
#
# Usage:
#   ./scripts/diagnose-cpu.sh                     # list running waft daemons
#   ./scripts/diagnose-cpu.sh waft-clock-daemon   # diagnose by name
#   ./scripts/diagnose-cpu.sh 12345               # diagnose by PID
#
# Requires: gdb, strace, ps, pgrep (standard Linux tools)
# Non-destructive: only reads state, does not modify the running process.
# The process is briefly paused during gdb attach (~1-2 seconds).
#
# If ptrace is blocked, run:
#   echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope

set -euo pipefail

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
CPU_THRESHOLD=5

# ---------------------------------------------------------------------------
# Argument parsing: name, PID, or list mode
# ---------------------------------------------------------------------------

TARGET="${1:-}"

if [[ -z "$TARGET" ]]; then
    echo "Running waft daemons:"
    echo ""
    pgrep -a -f 'waft-.*-daemon|waft$' 2>/dev/null | sort -k2 || echo "  (none found)"
    echo ""
    echo "Usage: $0 <daemon-name-or-pid>"
    echo "  e.g: $0 waft-clock-daemon"
    echo "       $0 waft-audio-daemon"
    echo "       $0 waft"
    echo "       $0 12345"
    exit 0
fi

# Resolve PID
if [[ "$TARGET" =~ ^[0-9]+$ ]]; then
    PID="$TARGET"
    DAEMON_NAME=$(ps -p "$PID" -o comm= 2>/dev/null || echo "unknown")
else
    DAEMON_NAME="$TARGET"
    # pgrep -x matches exact comm (process name, not full path)
    PID=$(pgrep -x "$DAEMON_NAME" 2>/dev/null || true)

    if [[ -z "$PID" ]]; then
        # Fall back to full cmdline match (handles path prefixes)
        PID=$(pgrep -f "$DAEMON_NAME" 2>/dev/null | head -1 || true)
    fi

    if [[ -z "$PID" ]]; then
        echo "ERROR: No process matching '${DAEMON_NAME}' found."
        echo ""
        echo "Running waft daemons:"
        pgrep -a -f 'waft-.*-daemon|waft$' 2>/dev/null | sort -k2 || echo "  (none found)"
        exit 1
    fi

    # If multiple PIDs matched, pick the first and warn
    if [[ $(echo "$PID" | wc -l) -gt 1 ]]; then
        echo "WARNING: multiple PIDs matched '${DAEMON_NAME}', using first:"
        echo "$PID"
        PID=$(echo "$PID" | head -1)
    fi
fi

LOGFILE="diagnose-cpu-${DAEMON_NAME}-${TIMESTAMP}.log"

log() {
    echo "$@" | tee -a "$LOGFILE"
}

section() {
    log ""
    log "================================================================"
    log "  $1"
    log "================================================================"
    log ""
}

log "Diagnosing: ${DAEMON_NAME} (PID ${PID})"
log "Log file:   ${LOGFILE}"

# ---------------------------------------------------------------------------
# Step 1: Confirm process is alive
# ---------------------------------------------------------------------------
section "Step 1: Process info"

if ! kill -0 "$PID" 2>/dev/null; then
    log "ERROR: PID $PID is not running."
    exit 1
fi

ps -p "$PID" -o pid,ppid,pcpu,pmem,vsz,rss,stat,comm,args 2>&1 | tee -a "$LOGFILE"

# ---------------------------------------------------------------------------
# Step 2: Per-thread CPU snapshot
# ---------------------------------------------------------------------------
section "Step 2: Per-thread CPU snapshot"

log "Threads sorted by CPU usage:"
ps -p "$PID" -L -o pid,lwp,pcpu,comm --sort=-pcpu 2>&1 | tee -a "$LOGFILE"

# Collect TIDs of hot threads (>CPU_THRESHOLD%)
HOT_TIDS=()
while IFS= read -r line; do
    tid=$(echo "$line" | awk '{print $2}')
    cpu=$(echo "$line" | awk '{print $3}')
    if awk "BEGIN {exit !($cpu > $CPU_THRESHOLD)}"; then
        HOT_TIDS+=("$tid")
    fi
done < <(ps -p "$PID" -L -o pid,lwp,pcpu,comm --sort=-pcpu --no-headers 2>/dev/null)

log ""
log "Hot threads (>${CPU_THRESHOLD}% CPU): ${HOT_TIDS[*]:-none}"

# ---------------------------------------------------------------------------
# Step 3: Child processes
# ---------------------------------------------------------------------------
section "Step 3: Child processes"

CHILDREN=$(pgrep -P "$PID" -a 2>/dev/null || true)
if [[ -n "$CHILDREN" ]]; then
    log "$CHILDREN"
else
    log "No child processes found."
fi

# ---------------------------------------------------------------------------
# Step 4: strace hot threads
# ---------------------------------------------------------------------------
section "Step 4: strace hot threads"

for TID in "${HOT_TIDS[@]}"; do
    log "--- strace syscall summary for TID $TID (2 seconds) ---"

    timeout 3 strace -p "$TID" -c -e trace=all 2>&1 | tee -a "$LOGFILE" &
    STRACE_PID=$!
    sleep 2
    kill "$STRACE_PID" 2>/dev/null || true
    wait "$STRACE_PID" 2>/dev/null || true

    log ""
    log "--- strace raw trace snippet for TID $TID (first 200 lines) ---"

    timeout 5 strace -p "$TID" -tt -T -e trace=all 2>&1 | head -200 | tee -a "$LOGFILE" || true

    log ""
done

if [[ ${#HOT_TIDS[@]} -eq 0 ]]; then
    log "No hot threads found — skipping strace."
fi

# ---------------------------------------------------------------------------
# Step 5: gdb all-thread backtraces
# ---------------------------------------------------------------------------
section "Step 5: gdb all-thread backtraces"

gdb -batch -ex "thread apply all bt" -p "$PID" 2>&1 | tee -a "$LOGFILE" || true

# ---------------------------------------------------------------------------
# Step 6: gdb focused backtrace of hot threads
# ---------------------------------------------------------------------------
section "Step 6: gdb focused backtrace of hot threads"

for TID in "${HOT_TIDS[@]}"; do
    log "--- Full backtrace for TID $TID ---"

    gdb -batch \
        -ex "thread find $TID" \
        -ex "bt full" \
        -p "$PID" 2>&1 | tee -a "$LOGFILE" || true

    log ""
done

if [[ ${#HOT_TIDS[@]} -eq 0 ]]; then
    log "No hot threads found — skipping focused backtrace."
fi

# ---------------------------------------------------------------------------
# Step 7: /proc info
# ---------------------------------------------------------------------------
section "Step 7: /proc info"

log "--- File descriptors (first 50) ---"
ls -la "/proc/$PID/fd/" 2>&1 | head -50 | tee -a "$LOGFILE"

log ""
log "--- FD count ---"
FD_COUNT=$(ls "/proc/$PID/fd/" 2>/dev/null | wc -l)
log "Total open file descriptors: $FD_COUNT"

log ""
log "--- Dead pipes / sockets ---"
for fd in /proc/"$PID"/fd/*; do
    target=$(readlink "$fd" 2>/dev/null || true)
    if [[ "$target" == *"(deleted)"* ]] || [[ "$target" == "pipe:"* ]]; then
        log "  $(basename "$fd") -> $target"
    fi
done

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------
section "Done"

log "Diagnostic output saved to: $LOGFILE"
log ""
log "Review hints:"
log "  - strace shows rapid poll/futex/epoll_wait with timeout=0 → busy-wait loop"
log "  - strace shows rapid recvmsg/sendmsg on a socket → message flood (check entity updates)"
log "  - gdb backtrace shows tokio park/poll loop → check for 0-timeout wakers or channel floods"
log "  - gdb shows ClaimTracker/handle_timeouts in hot path → claim loop firing too fast"
log "  - gdb shows notify() in tight loop → inotify feedback loop (Access events from own reads)"
log "  - DBus MessageStream in hot path → zbus error tight-loop"
log "  - Many open sockets with (deleted) pipes → runtime dropped receiver, sender spinning"
