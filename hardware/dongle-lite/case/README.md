# Dongle-Lite case

3D-printable two-piece enclosure for the `dongle-lite-1s4l` board, modeled
in [CadQuery](https://github.com/CadQuery/cadquery). Dimensions are pulled
from the PCB: 22 x 77 mm outline, both USB-C faces flush with the short
edges, USB-C shell (3.26 mm) is the tallest part.

```sh
uv run case.py
```

writes `output/`:

- `dongle-lite-case.step` — assembly (bottom + lid in place)
- `bottom.stl` — tray, prints as-is
- `lid.stl` — already flipped flat-side-down for printing

## Design

- Bottom tray: PCB drops in flat (single-sided board), 0.3 mm clearance per
  side. Four corner posts on the lid press it onto the floor.
- Lid: friction-fit perimeter lip plus four snap bumps that seat into
  grooves in the tray walls.
- Port bays: the receptacle sits flush with the board edge, 1.8 mm behind
  the outer face, so the end wall has to clear the cable's overmold, not
  just the plug shell — otherwise the plug stops ~2 mm short of seating.
  Each end is open 12.6 x 6.2 mm (flared to 14 mm over the outer 0.8 mm for
  a lead-in), which takes an overmold up to about 12 x 5.8 mm. The bay runs
  from the cavity floor up, so there's no thin ledge and nothing to bridge.
- Openings: 1.8 mm LED windows over D1 (PWR) and D2 (STAT), 2.4 mm
  paperclip hole over the BOOT button.
- Engraving: bold, 0.6 mm deep with flat bottoms, all reading across the
  width. HOST / TARGET with arrows at the port ends, wordmark centered in
  the clear band above the LEDs, and STAT / PWR / BOOT right-aligned beside
  their own holes. The lid prints top-face-down, so a filament swap at
  0.6 mm gives two-color inlaid text.
- Outer size: 26.2 x 81.2 x 8.8 mm, 1.8 mm walls.

The lid graphics are built in absolute coordinates and cut in a single
pass. Chaining `faces(">Z").workplane()` between engravings does not work:
the islands inside letter counters are coplanar with the lid top, so the
selector starts matching them and every later mark lands relative to
whichever letter it happened to pick.

Print at 0.2 mm layers, no supports. If the snap fit is too tight or
loose, tweak `LIP_CLR` / the snap sphere sizes in `case.py`.
