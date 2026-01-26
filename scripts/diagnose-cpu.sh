#!/usr/bin/env bash
#
# diagnose-cpu.sh — Attach to a running sacrebleui process and collect
# evidence to pinpoint 100% CPU usage.
#
# Requires: gdb, strace, ps, pgrep (standard Linux tools)
# Non-destructive: only reads state, does not modify the running process.
# The process is briefly paused during gdb attach (~1-2 seconds).
#
# If ptrace is blocked, run:
#   echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope

set -euo pipefail

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
LOGFILE="diagnose-cpu-${TIMESTAMP}.log"
CPU_THRESHOLD=5

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

# ---------------------------------------------------------------------------
# Step 1: Find the process
# ---------------------------------------------------------------------------
section "Step 1: Find sacrebleui process"

PID=$(pgrep -x sacrebleui 2>/dev/null || true)

if [[ -z "$PID" ]]; then
    log "ERROR: sacrebleui is not running."
    exit 1
fi

log "Found sacrebleui PID: $PID"

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
    # Compare as floating point: cpu > threshold
    if awk "BEGIN {exit !($cpu > $CPU_THRESHOLD)}"; then
        HOT_TIDS+=("$tid")
    fi
done < <(ps -p "$PID" -L -o pid,lwp,pcpu,comm --sort=-pcpu --no-headers 2>/dev/null)

log ""
log "Hot threads (>${CPU_THRESHOLD}% CPU): ${HOT_TIDS[*]:-none}"

# ---------------------------------------------------------------------------
# Step 3: Check child processes
# ---------------------------------------------------------------------------
section "Step 3: Child processes (is pactl subscribe alive?)"

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

    # Syscall summary (count mode)
    timeout 3 strace -p "$TID" -c -e trace=all 2>&1 | tee -a "$LOGFILE" &
    STRACE_PID=$!
    sleep 2
    kill "$STRACE_PID" 2>/dev/null || true
    wait "$STRACE_PID" 2>/dev/null || true

    log ""
    log "--- strace raw trace snippet for TID $TID (first 200 lines) ---"

    # Raw trace snippet
    timeout 5 strace -p "$TID" -tt -T -e trace=all 2>&1 | head -200 | tee -a "$LOGFILE" || true

    log ""
done

if [[ ${#HOT_TIDS[@]} -eq 0 ]]; then
    log "No hot threads found — skipping strace."
fi

# ---------------------------------------------------------------------------
# Step 5: gdb thread backtraces (all threads)
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
log "  - If strace shows millions of poll/futex/epoll_wait with 0 timeout → busy-wait"
log "  - If gdb backtrace shows a glib/tokio poll loop → glib↔tokio contention"
log "  - If pactl subprocess is missing → audio plugin sender dropped, possible spin"
log "  - If DBus MessageStream is in the hot path → zbus error tight-loop"
