#!/bin/sh
# DRC summary: counts by type, then the first few of each error. usage: gen/drc.sh [n]
cd "$(dirname "$0")/.."
kicad-cli pcb drc dongle-pro-v2.kicad_pcb -o /tmp/v2-drc.rpt --severity-all >/dev/null 2>&1
grep -E "^\[" /tmp/v2-drc.rpt | sed -E 's/\]:.*//; s/^\[//' | sort | uniq -c | sort -rn
grep -cE "^\[unconnected_items" /tmp/v2-drc.rpt
