"""Dongle-probe enclosure, generated with CadQuery.

Two pieces: a bottom tray the PCB drops into and a friction-fit lid with
posts that hold the board down. Dimensions come from layout/layout.kicad_pcb
(16 x 43.0 mm outline, USB-C flush with the +Y edge, board center at
kicad (108, 121.5)).

Run:  uv run case.py
Outputs STEP + STLs into ./output/.
"""

import os

import cadquery as cq

# --- PCB facts (from layout/layout.kicad_pcb) ---
PCB_W = 16.0
PCB_L = 43.0
PCB_T = 1.6
USB_SHELL_W = 8.94  # HRO TYPE-C-31-M-12, shell lip flush with the board edge
USB_SHELL_H = 3.26

# kicad -> case coords: x = kx - 108, y = -(ky - 121.5), z = 0 at PCB bottom
SW1 = (0.0, -(129.315 - 121.5))   # BOOT button, top-actuated
LED_PWR = (101.9 - 108.0, -(130.72 - 121.5))   # D1
LED_ACT = (114.5 - 108.0, -(130.72 - 121.5))   # D2
# J2 (2.54mm 2x3 shrouded IDC box header, housing 15.28 x 8.90 ending at the
# board edge, ~9mm tall so it pokes through the lid) lives at the -Y end; the
# TC2030-IDC-NL cable's IDC socket plugs in from above, so the lid is open
# over the whole housing
SWD_OPEN_X = (-7.95, 7.95)
SWD_OPEN_Y = (-21.8, -12.3)

# --- case parameters ---
CLR = 0.3  # PCB clearance per side
WALL = 1.8
FLOOR = 2.6  # THT_RELIEF pocket depth + 0.8 mm skin
LID_T = 1.6
THT_RELIEF = 1.8  # pocket depth under the through-hole solder tails
HEADROOM = 4.0  # cavity above PCB top (USB shell is 3.26)

CAV_W = PCB_W + 2 * CLR
CAV_L = PCB_L + 2 * CLR
CAV_D = PCB_T + HEADROOM
OUT_W = CAV_W + 2 * WALL
OUT_L = CAV_L + 2 * WALL
CORNER_R = 2.5

USB_CUT_W = 10.4
USB_CUT_Z0 = PCB_T - 0.3
USB_RECESS_W = 12.0  # shallow outer seat for the plug overmold

LIP_T = 1.2
LIP_H = 1.6
LIP_CLR = 0.15  # per side

SNAP_Y = 10.0  # snap bumps at y = +/-SNAP_Y on both long sides
SNAP_Z = CAV_D - LIP_H + 0.5

# 3 x 0.2 mm layers: the lid prints top-face-down, so a filament swap at
# 0.6 mm gives two-color inlaid text
TEXT_DEPTH = 0.6

# Arial Black, not Arial Bold, for stroke width: a 0.4 mm nozzle wants two
# perimeters per stem, and the inlay letters are free-standing 0.6 mm sticks
# that the slicer drops outright if they come in under a nozzle width. Arial
# Bold stems are 0.145 * size, Arial Black 0.222 * size -- 53% more stroke
# for 9% more width. See the dongle-lite case for the full argument.
FONT = "Arial Black"
STEM_RATIO = 0.222  # ink width of "I" per unit size, measured for FONT
MIN_STROKE = 0.65


def outer_profile(height, z0):
    return (
        cq.Workplane("XY", origin=(0, 0, z0))
        .box(OUT_W, OUT_L, height, centered=(True, True, False))
        .edges("|Z")
        .fillet(CORNER_R)
    )


# --- bottom tray ---
cutters = cq.Workplane("XY").box(CAV_W, CAV_L, CAV_D, centered=(True, True, False))
# relief pockets in the floor for the through-hole solder tails, which
# otherwise hold the board off the floor: the USB shell legs + locating pegs
# (x = +/-4.33 / +/-2.9, y 13.7..19.8) and J2's six pins (x = 0 / +/-2.54,
# y -18.3 / -15.8). The board still seats on the floor everywhere else.
for x0, x1, y0, y1 in ((-6.0, 6.0, 12.8, 20.8),     # J1 legs + pegs
                       (-4.5, 4.5, -20.0, -14.0)):  # J2 pins
    cutters = cutters.union(
        cq.Workplane("XY", origin=((x0 + x1) / 2, (y0 + y1) / 2, -THT_RELIEF))
        .box(x1 - x0, y1 - y0, THT_RELIEF + 0.1, centered=(True, True, False))
    )
# USB notch through the +Y wall, open to the top (the lid closes it)
cutters = cutters.union(
    cq.Workplane("XY", origin=(0, CAV_L / 2 + WALL / 2, USB_CUT_Z0))
    .box(USB_CUT_W, WALL + 2, CAV_D - USB_CUT_Z0 + 0.1, centered=(True, True, False))
)
# shallow recess on the outer face so the plug overmold can seat closer
cutters = cutters.union(
    cq.Workplane("XY", origin=(0, OUT_L / 2 - 0.5, USB_CUT_Z0 - 0.8))
    .box(USB_RECESS_W, 1.1, CAV_D, centered=(True, True, False))
)
# snap grooves in the long cavity walls
for sx in (1, -1):
    for sy in (1, -1):
        cutters = cutters.union(
            cq.Workplane("XY")
            .sphere(0.75)
            .translate((sx * CAV_W / 2, sy * SNAP_Y, SNAP_Z))
        )
bottom = outer_profile(FLOOR + CAV_D, -FLOOR).cut(cutters, clean=False)
bottom = bottom.edges("<Z").fillet(0.5)

# --- lid ---
lid = outer_profile(LID_T, CAV_D)
lip = (
    cq.Workplane("XY", origin=(0, 0, CAV_D - LIP_H))
    .rect(CAV_W - 2 * LIP_CLR, CAV_L - 2 * LIP_CLR)
    .rect(CAV_W - 2 * LIP_CLR - 2 * LIP_T, CAV_L - 2 * LIP_CLR - 2 * LIP_T)
    .extrude(LIP_H)
)
# clear the lip at the USB notch
lip = lip.cut(
    cq.Workplane("XY", origin=(0, CAV_L / 2 - 1, CAV_D - LIP_H))
    .box(USB_CUT_W + 1.6, 4, LIP_H, centered=(True, True, False))
)
lid = lid.union(lip)
# hold-down posts: full corners at the USB end (outside the plug path),
# slim posts just north of the J2 housing at the -Y end (between the LED
# columns and the series-resistor row; J2's six THT joints anchor that end)
POSTS = [(-7.4, 20.5, 1.4, 2.2), (7.4, 20.5, 1.4, 2.2),
         (-7.15, -11.6, 0.9, 1.4), (3.7, -11.6, 1.0, 1.4)]
for px, py, pw, pd in POSTS:
    lid = lid.union(
        cq.Workplane("XY", origin=(px, py, PCB_T))
        .box(pw, pd, CAV_D - PCB_T, centered=(True, True, False))
    )
# snap bumps on the lip
for sx in (1, -1):
    for sy in (1, -1):
        lid = lid.union(
            cq.Workplane("XY")
            .sphere(0.6)
            .translate((sx * (CAV_W / 2 - LIP_CLR), sy * SNAP_Y, SNAP_Z))
        )
lid = lid.edges(">Z").fillet(0.5)
# SWD opening: J2 header + TC2030 IDC socket + JP1 access (through lid + lip)
ox = (SWD_OPEN_X[0] + SWD_OPEN_X[1]) / 2
oy = (SWD_OPEN_Y[0] + SWD_OPEN_Y[1]) / 2
lid = lid.cut(
    cq.Workplane("XY", origin=(ox, oy, CAV_D - LIP_H))
    .box(SWD_OPEN_X[1] - SWD_OPEN_X[0], SWD_OPEN_Y[1] - SWD_OPEN_Y[0],
         LIP_H + LID_T + 0.2, centered=(True, True, False)),
    clean=False,
)

# --- lid graphics ---
# Built in absolute coordinates and cut in a single pass. Re-selecting
# faces(">Z") between engravings does not work: the islands inside letter
# counters are coplanar with the lid top, so the selector starts matching
# them and every later mark lands relative to whichever letter it picked.
# That is exactly how the previous revision ended up stamping BOOT through
# the middle of the wordmark.
#
# The three hole labels read along the length, the wordmark across it.
#
# This lid is only 20.2 mm wide and the LEDs sit at x = -6.1 / +6.5, so an
# across-width label centred on an LED runs off the case, and the 3 mm
# gutter between an LED and the wall is too narrow to put one beside it.
# Lengthwise they get free run and stay fat. That leaves the wordmark, which
# would want the centre column -- exactly where the BOOT button is -- so it
# goes across the width instead, up in the clear band. Cramming it in
# lengthwise costs more than it buys: BOOT then has to squeeze in sideways
# between PWR and ACT with half-millimetre gaps, and the whole lower third
# turns into a jumble of two reading directions.
#
# PWR / BOOT / ACT are one row with flush bottoms. BOOT cannot start as low
# as the LED labels -- its button is 1.8 mm further up the board and the
# label would land in the hole -- so the row bottom is set by BOOT and the
# LED labels ride up to meet it.
WORDMARK_SIZE = 3.1  # stems 0.69 mm
LABEL_SIZE = 3.5  # stems 0.78 mm

WORDMARK_Y = 12.1  # centred in the band between the label row and the arrow
LABEL_BOTTOM = -6.0

ARROW_BASE = 19.6
ARROW_TIP = 21.8
ARROW_W = 5.0

TOP_Z = CAV_D + LID_T


def place(shape, cx=None, cy=None, ymin=None):
    """Anchor a mark by its ink bounding box.

    cadquery centres text on font metrics, not on the ink it actually draws,
    so ascenders and side bearings leave the glyphs a fraction of a mm off
    from wherever you asked for. That is invisible on a big label and very
    visible when two labels are meant to line up with each other or sit on
    the lid centreline, so every mark gets snapped to its real bounds.
    """
    b = shape.val().BoundingBox()
    dx = 0.0 if cx is None else cx - (b.xmin + b.xmax) / 2
    if ymin is not None:
        dy = ymin - b.ymin
    elif cy is not None:
        dy = cy - (b.ymin + b.ymax) / 2
    else:
        dy = 0.0
    return shape.translate((dx, dy, 0))


def engrave(label, size, rot=0):
    """Label as a solid, to be positioned with place()."""
    t = cq.Workplane("XY", origin=(0, 0, TOP_Z - TEXT_DEPTH)).text(
        label, size, TEXT_DEPTH, font=FONT, kind="regular"
    )
    return t.rotate((0, 0, 0), (0, 0, 1), rot) if rot else t


def arrow():
    """Solid triangle pointing at the USB port on the +Y end."""
    pts = [(-ARROW_W / 2, ARROW_BASE), (ARROW_W / 2, ARROW_BASE),
           (0.0, ARROW_TIP)]
    return (
        cq.Workplane("XY", origin=(0, 0, TOP_Z - TEXT_DEPTH))
        .polyline(pts)
        .close()
        .extrude(TEXT_DEPTH)
    )


def thru_hole(x, y, d):
    """Window through the lid slab only, stopping at its underside.

    Overshooting downward would be the obvious way to guarantee a clean
    through-cut, but the friction lip hangs off that underside and its inner
    face is at x = +/-6.95 -- closer in than the outer edge of the ACT window
    (x = 7.4) and of the PWR window (x = -7.0). A cutter that runs past
    z = CAV_D chews a notch out of the lip at both LEDs. Starting exactly at
    the underside keeps the cut clear of the lip's z range entirely; the
    overshoot goes upward instead, into open air above the lid.
    """
    return (
        cq.Workplane("XY", origin=(x, y, CAV_D))
        .circle(d / 2)
        .extrude(LID_T + 1)
    )


# The graphics are kept separate from the holes: they are both the pockets
# cut into the lid and, exported on their own, the white inlay that drops
# back into them.
inlay = place(engrave("restorekit", WORDMARK_SIZE), cx=0, cy=WORDMARK_Y)
for m in (
    arrow(),
    # each centred on its own hole, bottoms flush so the row reads as a set
    place(engrave("PWR", LABEL_SIZE, rot=90), cx=LED_PWR[0], ymin=LABEL_BOTTOM),
    place(engrave("BOOT", LABEL_SIZE, rot=90), cx=SW1[0], ymin=LABEL_BOTTOM),
    place(engrave("ACT", LABEL_SIZE, rot=90), cx=LED_ACT[0], ymin=LABEL_BOTTOM),
):
    inlay = inlay.union(m)

marks = inlay
for h in (
    thru_hole(*LED_PWR, 1.8),
    thru_hole(*LED_ACT, 1.8),
    thru_hole(*SW1, 2.6),
):
    marks = marks.union(h)
lid = lid.cut(marks, clean=False)

# --- stroke check ---
# If FONT is missing the text still renders, just in whatever the fallback
# is, and a lighter fallback silently puts us back to unprintable strokes.
# Measure a stem instead of trusting the font lookup.
stem = cq.Workplane("XY").text("I", 1.0, 0.1, font=FONT, kind="regular")
stem = stem.val().BoundingBox().xlen
for name, size in (("wordmark", WORDMARK_SIZE),
                   ("hole labels", LABEL_SIZE)):
    w = stem * size
    flag = "" if w >= MIN_STROKE else f"  <-- under {MIN_STROKE} mm"
    print(f"{name:12} size {size}  stems {w:.2f} mm{flag}")
if stem * min(WORDMARK_SIZE, LABEL_SIZE) < MIN_STROKE:
    raise SystemExit(
        f"stroke too thin: {FONT!r} measured {stem:.3f} per unit size, "
        f"expected ~{STEM_RATIO}. Is the font installed?"
    )

# --- export ---
out = os.path.join(os.path.dirname(__file__), "output")
os.makedirs(out, exist_ok=True)

assembly = (
    cq.Assembly()
    .add(bottom, name="bottom", color=cq.Color(0.25, 0.25, 0.28))
    .add(lid, name="lid", color=cq.Color(0.85, 0.85, 0.87))
    .add(inlay, name="inlay", color=cq.Color(1.0, 1.0, 1.0))
)
assembly.export(os.path.join(out, "dongle-probe-case.step"))

cq.exporters.export(bottom, os.path.join(out, "bottom.stl"))
# flip the lid so its flat top sits on the print bed. The inlay gets the same
# transform, so loading it as a second part lands it in its pockets with no
# repositioning.
flip = lambda w: w.rotate((0, 0, 0), (0, 1, 0), 180)
cq.exporters.export(flip(lid), os.path.join(out, "lid.stl"))
cq.exporters.export(flip(inlay), os.path.join(out, "lid-inlay.stl"))

print("wrote", out)
print(f"outer: {OUT_W} x {OUT_L} x {FLOOR + CAV_D + LID_T} mm")
