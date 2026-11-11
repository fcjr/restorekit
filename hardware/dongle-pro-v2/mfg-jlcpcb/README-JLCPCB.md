# Ordering from JLCPCB

Common to both options: 4 layers, 1.6 mm, stackup JLC04161H-7628, impedance
control on, Economic PCBA, top side only. Check every part's rotation in the
placement preview before paying; none of them have been verified.

Both USB-C receptacles hang 1.3 mm over the top and bottom board edges. That is
the constraint behind both layouts below: rails can only go on the long sides.

## Option 1: our panel (KiKit)

Files in `../panel/mfg-jlcpcb/`: `gerbers.zip`, `bom.csv`, `pos.csv`.

- 81.1 x 48.1 mm, three boards side by side, 2 mm apart.
- 5 mm rails left and right with three tooling holes and three fiducials.
- Two 3 mm mouse-bite tabs on each long edge. Top and bottom are open.
- Every part has its own designator (`C1_1`, `C1_2`, `C1_3`), so the placement
  file has no repeats and uploads under Delivery Format "Single PCB" as one
  81 x 48 mm board. JLCPCB's review can still see three boards in the gerbers
  and may reclassify it as "Panel by Customer"; if it does, pick that with
  1 design and 3 boards per panel. Quantity is panels: 5 is 15 dongles.
- Regenerate with `gen/panel.sh` after any board change.

## Option 2: let JLCPCB panel it

Files in this directory: `dongle-pro-v2-gerbers-jlcpcb.zip`, `bom.csv`,
`cpl.csv`. This is the single 21 x 48 mm board.

- Delivery Format "Panel by JLCPCB", 3 columns x 1 row, edge rails on the left
  and right.
- Their panels are V-cut. One row keeps the receptacles clear, but check the
  form accepts a 48 mm tall panel; if it insists on 70 mm, use option 1.
- Do not pick two rows: the receptacles of stacked boards would collide.
- If you order single boards with no panel, expect them to add rails for
  assembly because of the overhanging connectors.
