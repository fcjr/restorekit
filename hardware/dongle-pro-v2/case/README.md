# Dongle-Pro v2 case

Two-piece printed case for the `dongle-pro-v2` board. No screws, no
supports, no paperclip. 25.2 x 49.9 x 8.8 mm.

```sh
uv run case.py    # output/: STEP assembly + STLs
uv run check.py   # fit checks against ../mfg-jlcpcb/dongle-pro-v2.step
uv run check.py --pinhole
```

There are two lids. `lid.stl` has the flex-tab BOOT button. `lid-pinhole.stl`
has a 2.4 mm paperclip hole over the button instead, like the Lite. It's
sturdier and you can't bump BOOT by accident. Same tray for both.

## Printing

- `tray.stl` prints as-is.
- `lid.stl` is already flipped top-face-down.
- `lid-inlay.stl` is the white bit. Load it as a second *part* of the lid
  (not a separate object) and it lands in its pockets. On an AMS/MMU you get
  flush white text and white LED diffusers. No MMU? Skip it: the text is
  engraved and the LED windows are open holes.

0.2 mm layers, PETG or PLA. PETG is nicer for the button.

## Assembly

1. Drop the board in, components up. It sits flat on the floor.
2. Press the lid on until it clicks. It only goes on one way (HOST end with
   HOST end); a rib in the tray stops it backwards.
3. To open, put a fingernail in the notch on the side without the LEDs.

## What's in it

- BOOT is a flex tab in the lid with a nub 0.25 mm over SW1. Press the ring.
- PWR and STAT windows sit right over D1 and D2.
- The ports get a snug stadium hole, 0.25 mm around the shell. The receptacles
  stick 0.53 mm past the board, so the 0.8 mm end walls wrap that and leave the
  mouth 0.42 mm behind the face. The tray is notched above each hole and a
  tongue on the lid fills the top, so nothing bridges.
- The lid snaps on with four 8 mm ridges into grooves in the tray walls, kept
  clear of every cut in the lip. Round bumps were a single point of contact
  and closed without much of a click.
- Four blocks on the lid pin the board to the floor at its corners and back up
  the thin end walls.

`check.py` loads the board STEP and checks that nothing touches a part, the
lid seats, a backwards lid doesn't, the nub gap, flex strain, the snap bite
and the solid lip around each snap, how far the mouth sits behind the face,
that the receptacles clear their holes with the board slid either way, and
that no label runs into a hole or another label.

The receptacle 3D model in the lib sits 1.6 mm too low (shell through a
board with no cutout). `case.py` and `check.py` shift it up onto the board,
which a printed case has confirmed.
