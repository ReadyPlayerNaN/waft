#!/usr/bin/env python3
"""List all calendar sources known to Evolution Data Server (EDS) via D-Bus.

Requires: gdbus
Useful for debugging the agenda plugin.
"""

import re
import subprocess
import sys

EDS_DEST = "org.gnome.evolution.dataserver.Sources5"
EDS_PATH = "/org/gnome/evolution/dataserver/SourceManager"


def main():
    result = subprocess.run(
        [
            "gdbus", "call", "--session",
            "--dest", EDS_DEST,
            "--object-path", EDS_PATH,
            "--method", "org.freedesktop.DBus.ObjectManager.GetManagedObjects",
        ],
        capture_output=True,
        text=True,
    )

    if result.returncode != 0:
        print(f"Failed to query EDS: {result.stderr.strip()}", file=sys.stderr)
        sys.exit(1)

    data = result.stdout

    # Split on source object paths to parse each source independently.
    # This avoids misaligning UID/Data pairs across different sources.
    parts = re.split(
        r"'/org/gnome/evolution/dataserver/SourceManager/Source_\d+': ", data
    )

    calendars = []
    for part in parts:
        uid_m = re.search(r"'UID': <'([^']+)'>", part)
        if not uid_m:
            continue
        uid = uid_m.group(1)

        if "[Calendar]" not in part:
            continue

        data_m = re.search(r"'Data': <'(.*?)'>", part)
        if not data_m:
            continue
        source_data = data_m.group(1)

        name_m = re.search(r"DisplayName=(.+?)(?:\\n)", source_data)
        name = name_m.group(1) if name_m else uid
        backend_m = re.search(r"BackendName=(\w+)", source_data)
        backend = backend_m.group(1) if backend_m else ""
        color_m = re.search(r"Color=(#[0-9a-fA-F]+)", source_data)
        color = color_m.group(1) if color_m else ""

        calendars.append((name, uid, backend, color))

    if not calendars:
        print("No calendar sources found in EDS.")
        sys.exit(0)

    print(f"Found {len(calendars)} calendar source(s):")
    print()
    for name, uid, backend, color in calendars:
        info = [f"uid={uid}"]
        if backend:
            info.append(f"backend={backend}")
        if color:
            info.append(f"color={color}")
        print(f"  {name}")
        print(f"    {'  '.join(info)}")
        print()


if __name__ == "__main__":
    main()
