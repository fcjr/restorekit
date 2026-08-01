# Dongle-Lite case

3D-printable two-piece enclosure for the `dongle-lite-1s4l` board, modeled
in [CadQuery](https://github.com/CadQuery/cadquery). Dimensions are pulled
from the PCB: 22 x 77 mm outline, both USB-C faces flush with the short
edges, USB-C shell (3.26 mm) is the tallest part.

```sh
uv run case.py
```

writes `output/`:

- `dongle-lite-case.step` — assembly (bottom + lid + inlay in place)
- `bottom.stl` — tray, prints as-is
- `lid.stl` — already flipped flat-side-down for printing
- `lid-inlay.stl` — the white text inlay, same orientation as `lid.stl`

## Design

- Bottom tray: PCB drops in flat (single-sided board), 0.3 mm clearance per
  side. Four corner posts on the lid press it onto the floor.
- Lid: friction-fit perimeter lip plus four snap bumps that seat into
  grooves in the tray walls.
- Port bays: the receptacle sits flush with the board edge, 1.8 mm behind
  the outer face, so the end wall has to clear the cable's overmold, not
  just the plug shell — otherwise the plug stops ~2 mm short of seating.
  Each end is open 15.0 x 6.8 mm all the way through, with an 18.6 mm
  rounded relief pocket 1.0 mm into the outer face for a chunky overmold to
  nose into. That takes an overmold up to about 14.6 x 6.3 mm, or 16 mm
  wide at the mouth. The bay is open to the top of the tray, so there's
  nothing to bridge.

  The top is the binding constraint, not the sides: the board sits on the
  floor, which puts the connector centre 4.83 mm off the bottom of an
  8.8 mm case, leaving 0.8 mm of lid above the bay and 1.2 mm of floor
  below. `PORT_Z1` is what buys taller overmolds, at the cost of that roof.
- Openings: 1.8 mm LED windows over D1 (PWR) and D2 (STAT), 2.4 mm
  paperclip hole over the BOOT button.
- Engraving: bold, 0.6 mm deep with flat bottoms, all reading across the
  width. HOST / TARGET with arrows at the port ends, wordmark centered in
  the clear band above the LEDs, and STAT / PWR / BOOT right-aligned beside
  their own holes.

## White text inlay

`lid-inlay.stl` is exactly the volume of the engraved pockets, exported in
the same orientation and position as `lid.stl` — union the two and you get
a flat-topped lid, with no interference and no gap.

With a multi-material printer (MMU / AMS / toolchanger): load `lid.stl`,
add `lid-inlay.stl` to it as a second *part* of the same object (not a
separate object, so it isn't re-centred), assign white to the inlay, print
the lid top-face-down. The text comes out flush and white.

Single extruder, no MMU: print `lid-inlay.stl` on its own in white, 0.6 mm
of flat letters, and glue them in. Fiddly on the small labels.

What does *not* work is a plain filament swap. The lid prints top-face-down
and the text is recessed, so the letters are voids in the first three
layers: swapping at 0.6 mm colours the top face and leaves the letters in
the body colour — the inverse of what you want. Getting white letters out
of one extruder would mean embossing the text proud of the lid instead of
engraving it, which trades the flush finish for something that scuffs.
- Outer size: 26.2 x 81.2 x 8.8 mm, 1.8 mm walls.

The lid graphics are built in absolute coordinates and cut in a single
pass. Chaining `faces(">Z").workplane()` between engravings does not work:
the islands inside letter counters are coplanar with the lid top, so the
selector starts matching them and every later mark lands relative to
whichever letter it happened to pick.

Print at 0.2 mm layers, no supports. If the snap fit is too tight or
loose, tweak `LIP_CLR` / the snap sphere sizes in `case.py`.
