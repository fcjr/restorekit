#!/bin/sh
# Render the board to /tmp/v2pd-<side>.png for a quick look. usage: gen/view.sh [layers] [name]
cd "$(dirname "$0")/.."
L="${1:-F.Cu,B.Cu,F.Fab,F.Courtyard,Edge.Cuts}"; N="${2:-top}"
kicad-cli pcb export svg dongle-pro-v2-pd.kicad_pcb -o /tmp/v2pd-$N.svg --layers "$L" --mode-single --page-size-mode 2 --exclude-drawing-sheet >/dev/null 2>&1
rsvg-convert -z 14 -b white /tmp/v2pd-$N.svg -o /tmp/v2pd-$N.png && echo /tmp/v2pd-$N.png
