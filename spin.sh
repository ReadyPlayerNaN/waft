#!/usr/bin/env sh
set -eu

PID="${1:-}"
if [ -z "${PID}" ]; then
  PID="$(pidof -s sacrebleui || true)"
fi

if [ -z "${PID}" ]; then
  echo "Could not find sacrebleui PID. Pass PID explicitly: $0 <pid>" >&2
  exit 1
fi

OUT="/tmp/sacrebleui-glib-spin.$PID.out"
GDBCMDS="$(mktemp -t sacrebleui-gdb.XXXXXX)"

cat > "$GDBCMDS" <<'GDB'
set pagination off
set confirm off
set breakpoint pending on
set print thread-events off
set print inferior-events off
set verbose off

printf "Attached. Capturing GLib source scheduling via g_source_set_ready_time(ready_time==0) (bounded output)...\n"
set $hits = 0
set $limit = 200

define _print_ready_source
  # We break on g_source_set_ready_time(GSource *source, gint64 ready_time).
  # On x86_64 SysV ABI:
  #   source     -> $rdi
  #   ready_time -> $rsi
  set $src = (void*)$rdi
  set $ready = (long)$rsi

  set $nm = (char*) g_source_get_name((void*)$src)
  if $nm == 0
    printf "  source=%p name=<null> ready_time=%ld\n", $src, $ready
  else
    printf "  source=%p name=%s ready_time=%ld\n", $src, $nm, $ready
  end
end

# Busy-spin cases often come from sources that keep being scheduled "ready immediately".
# We therefore only log when ready_time == 0 to keep output focused.
break g_source_set_ready_time
commands
  silent
  set $ready = (long)$rsi
  if $ready == 0
    set $hits = $hits + 1
    printf "\n== hit g_source_set_ready_time ready_time==0 #%d ==\n", $hits
    _print_ready_source
    bt 12

    if $hits >= $limit
      printf "\nReached $limit=%d hits; detaching.\n", $limit
      detach
      quit
    end
  end
  continue
end

info breakpoints
continue
GDB

# Run gdb non-interactively; stop it yourself with Ctrl-C if needed.
sudo gdb -q -p "$PID" -x "$GDBCMDS" | tee "$OUT"

rm -f "$GDBCMDS"
echo "Saved output to $OUT"
