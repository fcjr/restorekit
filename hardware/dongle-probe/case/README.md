# dongle-probe case

Snap-fit two-piece enclosure (bottom tray + lid) generated with
[CadQuery](https://github.com/cadquery/cadquery).

```sh
uv run case.py
```

Outputs to `output/`:

- `dongle-probe-case.step` — full assembly (bottom + lid + inlay in place)
- `bottom.stl` — tray, prints as-is
- `lid.stl` — print-ready, pre-flipped flat side down
- `lid-inlay.stl` — the white text inlay, same orientation as `lid.stl`

Outer size: 20.2 x 47.2 x 9.8 mm, 1.8 mm walls.

Features:

- USB-C notch at the top end, with an outer overmold recess (the shell sits
  1.4 mm inside the board edge, so bulky cable overmolds can enter the notch)
- open bay at the bottom end for the 2x3 2.54 mm shrouded SWD box header —
  the TC2030 IDC socket seats from above — with JP1 (VTREF jumper) reachable
  through the same opening
- BOOT button hole, PWR/ACT LED windows, engraved labels
- friction-fit lip with snap bumps, hold-down posts pressing the PCB into
  the tray
- 1.8 mm relief pockets in the tray floor under the USB-C shell legs and the
  SWD header's solder tails, so the board seats flat instead of riding on
  the through-hole joints (0.8 mm of floor remains below them)

Print the tray cavity-up and the lid as exported; no supports needed.

## Engraving

Arial Black, 0.6 mm deep with flat bottoms. The font is picked for stroke
width, not for looks: a 0.4 mm nozzle wants at least 0.8 mm of stroke so each
stem gets two perimeters instead of one skinny thin-wall extrusion, and the
inlay letters are free-standing sticks 0.6 mm tall that the slicer drops
outright if they come in under a nozzle width. Arial Bold stems are
0.145 × size against Arial Black's 0.222 × size — 53% more stroke for 9% more
width. `case.py` measures a stem before exporting and refuses to write the
STLs if it lands under `MIN_STROKE`, so a missing Arial Black silently
falling back to a lighter face can't put thin text back.

This lid is narrow enough that it can't quite reach 0.8 mm everywhere the way
the dongle-lite one does — 20.2 mm of width against 26.2 mm. The hole labels
come out at 0.78 mm and the wordmark at 0.69 mm.

Layout: PWR / BOOT / ACT read along the length, the wordmark reads across it.
The LEDs sit at x = -6.1 / +6.5 on a lid that is only +/-10.1 wide, so an
across-width label centred on an LED runs off the case, and the 3 mm gutter
between an LED and the wall is too narrow to put one beside it. Lengthwise
the labels get free run and stay fat. That leaves the wordmark wanting the
centre column, which is where the BOOT button already is, so it goes across
the width up in the clear band instead. The three hole labels share one row
with flush bottoms; BOOT sets the row bottom, because its button is 1.8 mm
further up the board than the LEDs and the label would otherwise land in its
own hole.

Marks are built in absolute coordinates and cut in a single pass. Chaining
`faces(">Z").workplane()` between engravings does not work: the islands
inside letter counters are coplanar with the lid top, so the selector starts
matching them and every later mark lands relative to whichever letter it
happened to pick. An earlier revision did exactly that and stamped BOOT
through the middle of the wordmark.

Each mark is also snapped to its own ink bounding box before placement.
CadQuery centres text on font metrics rather than on the ink it actually
draws, which leaves glyphs a fraction of a millimetre off — invisible on a
lone label, very visible when two labels are meant to line up with each other
or sit on the lid centreline.

## White text inlay

`lid-inlay.stl` is exactly the volume of the engraved pockets, exported in
the same orientation and position as `lid.stl` — union the two and you get a
flat-topped lid, with no interference and no gap.

With a multi-material printer (MMU / AMS / toolchanger): load `lid.stl`, add
`lid-inlay.stl` to it as a second *part* of the same object (not a separate
object, so it isn't re-centred), assign white to the inlay, print the lid
top-face-down. The text comes out flush and white.

Single extruder, no MMU: print `lid-inlay.stl` on its own in white, 0.6 mm of
flat letters, and glue them in. Fiddly — 22 loose pieces — but the strokes
are wide enough that they survive the slicer and tweezers.

What does *not* work is a plain filament swap. The lid prints top-face-down
and the text is recessed, so the letters are voids in the first three layers:
swapping at 0.6 mm colours the top face and leaves the letters in the body
colour — the inverse of what you want.
