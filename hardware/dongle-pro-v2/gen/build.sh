#!/bin/sh
# Full regen: schematic -> netlist check -> placement -> routing -> pours -> DRC summary.
set -e
cd "$(dirname "$0")/.."
KPY=/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/Current/bin/python3
python3 gen/board.py | tail -1
kicad-cli sch export netlist --format kicadsexpr dongle-pro-v2.kicad_sch -o /tmp/v2.net >/dev/null
python3 gen/netcheck.py /tmp/v2.net | tail -1
for s in pcb dump route zones; do $KPY -u gen/$s.py 2>&1 | grep -v traits | tail -2; done
[ -f gen/silk.py ] && $KPY -u gen/silk.py 2>&1 | grep -v traits | tail -1
gen/drc.sh | head -8
