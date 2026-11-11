#!/bin/sh
# 3-up panel for JLCPCB assembly: boards side by side, 5 mm rails left and right
# (the receptacles overhang the top and bottom edges, so those stay open),
# mouse-bite tabs on the long edges. Needs KiKit:
#   KPY -m venv --system-site-packages ~/.local/share/kikit-venv && ~/.local/share/kikit-venv/bin/pip install kikit
set -e
cd "$(dirname "$0")/.."
K="${KIKIT:-$HOME/.local/share/kikit-venv/bin/kikit}"
mkdir -p panel
$K panelize \
 --layout 'grid; rows: 1; cols: 3; space: 2mm' \
 --tabs 'fixed; hcount: 2; vcount: 0; width: 3mm' \
 --cuts 'mousebites; drill: 0.5mm; spacing: 0.8mm; offset: 0mm; prolong: 0.5mm' \
 --framing 'railslr; width: 5mm; space: 2mm' \
 --tooling '3hole; hoffset: 2.5mm; voffset: 3mm; size: 1.5mm' \
 --fiducials '3fid; hoffset: 3.85mm; voffset: 7mm; coppersize: 1mm; opening: 2mm' \
 --post 'millradius: 1mm; refillzones: true' \
 dongle-pro-v2.kicad_pcb panel/dongle-pro-v2-panel.kicad_pcb 2>&1 | grep -iE "error|warn" || true
rm -rf panel/mfg-jlcpcb
$K fab jlcpcb --assembly --schematic dongle-pro-v2.kicad_sch --field LCSC --no-drc \
 panel/dongle-pro-v2-panel.kicad_pcb panel/mfg-jlcpcb 2>&1 | grep -iE "error|warn" || true
rm -f panel/*.lck panel/*.kicad_prl
kicad-cli pcb drc panel/dongle-pro-v2-panel.kicad_pcb -o /tmp/v2-panel-drc.rpt --severity-error | head -2
