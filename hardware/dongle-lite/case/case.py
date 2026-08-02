"""Dongle-Lite enclosure, generated with CadQuery.

Two pieces: a bottom tray the PCB drops into and a friction-fit lid with
corner posts that hold the board down. Dimensions come from
dongle-lite-1s4l.kicad_pcb (22 x 77 mm outline, USB-C faces flush with the
short edges, board center at kicad (107, 137.5)).

Run:  uv run case.py
Outputs STEP + STLs into ./output/.
"""

import os

import cadquery as cq

# --- PCB facts (from dongle-lite-1s4l.kicad_pcb) ---
PCB_W = 22.0
PCB_L = 77.0
PCB_T = 1.6
USB_SHELL_W = 8.94  # HRO TYPE-C-31-M-12
USB_SHELL_H = 3.26

# kicad -> case coords: x = kx - 107, y = -(ky - 137.5), z = 0 at PCB bottom
SW1 = (112.4356 - 107.0, -(160.0356 - 137.5))  # BOOT button, top-actuated
LED1 = (113.5726 - 107.0, -(139.9263 - 137.5))
LED2 = (113.5214 - 107.0, -(133.2004 - 137.5))

# --- case parameters ---
CLR = 0.3  # PCB clearance per side
WALL = 1.8
FLOOR = 1.6
LID_T = 1.6
HEADROOM = 4.0  # cavity above PCB top (USB shell is 3.26)

CAV_W = PCB_W + 2 * CLR
CAV_L = PCB_L + 2 * CLR
CAV_D = PCB_T + HEADROOM
OUT_W = CAV_W + 2 * WALL
OUT_L = CAV_L + 2 * WALL
OUT_H = FLOOR + CAV_D + LID_T
CORNER_R = 2.5
TOP_FILLET = 0.5

# Port bays. The receptacle sits flush with the PCB edge, 1.8 mm behind the
# outer face, so a plug can only seat if the end wall is fully open in front
# of it -- a plain shell-sized notch would leave the cable's overmold hitting
# the case ~2 mm short. The bay is sized for the overmold instead of the
# shell, and a shallow recess around it in the outer face gives a chunky
# overmold somewhere to nose into.
#
# The top is the binding constraint, not the sides: the board sits on the
# floor, so the connector centre is 4.83 mm off the bottom of an 8.8 mm case
# and there is only ~1 mm of lid left above the bay. Raising PORT_Z1 further
# is what buys taller overmolds, at the cost of that roof.
PORT_W = 15.0
PORT_Z0 = -0.4  # 0.4 below the cavity floor, leaving 1.2 mm of tray floor
PORT_Z1 = 6.4  # leaves 0.8 mm of lid above the bay
PORT_RECESS_W = 18.6  # relief pocket in the outer face, around the bay
PORT_RECESS_D = 1.0  # of the 1.8 mm wall
PORT_RECESS_R = 1.2

LIP_T = 1.2
LIP_H = 1.6
LIP_CLR = 0.15  # per side

POST = 2.2  # square corner posts on the lid that press the PCB down
POST_C = (CAV_W / 2 - LIP_CLR - POST / 2,
          CAV_L / 2 - LIP_CLR - POST / 2)

SNAP_Y = 20.0  # snap bumps at y = +/-SNAP_Y on both long sides
SNAP_Z = CAV_D - LIP_H + 0.5

# 3 x 0.2 mm layers: the lid prints top-face-down, so a filament swap at
# 0.6 mm gives two-color inlaid text
TEXT_DEPTH = 0.6


def outer_profile(height, z0):
    return (
        cq.Workplane("XY", origin=(0, 0, z0))
        .box(OUT_W, OUT_L, height, centered=(True, True, False))
        .edges("|Z")
        .fillet(CORNER_R)
    )


def port_cutter(sign):
    """Plug bay through one end wall, plus its relief pocket.

    Built with the plug axis along +Z (z = 0 at the outer face, +Z inward)
    then rotated onto +/-Y, so the rounded pocket never has to be filleted on
    a tilted workplane -- that is what crashes OCC's cleanup.
    """
    h = PORT_Z1 - PORT_Z0
    bay = cq.Workplane("XY", origin=(0, 0, -1.5)).box(
        PORT_W, h, 4.5, centered=(True, True, False)
    )
    recess = (
        cq.Workplane("XY", origin=(0, 0, -1.5))
        .box(PORT_RECESS_W, h, 1.5 + PORT_RECESS_D, centered=(True, True, False))
        .edges("|Z")
        .fillet(PORT_RECESS_R)
    )
    return (
        bay.union(recess, clean=False)
        .val()
        .rotate((0, 0, 0), (1, 0, 0), sign * 90)
        .translate((0, sign * OUT_L / 2, (PORT_Z0 + PORT_Z1) / 2))
    )


# --- bottom tray ---
cutters = cq.Workplane("XY").box(CAV_W, CAV_L, CAV_D, centered=(True, True, False))
for sign in (1, -1):
    cutters = cutters.union(port_cutter(sign))
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
# clear the lip where the plug bays are -- it would otherwise sit on top of
# the USB connector body and on the leading edge of the overmold
for sign in (1, -1):
    lip = lip.cut(
        cq.Workplane("XY", origin=(0, sign * (CAV_L / 2 - 1), CAV_D - LIP_H))
        .box(PORT_W + 0.6, 4, LIP_H, centered=(True, True, False))
    )
lid = lid.union(lip)
# corner posts down to the PCB top
for sx in (1, -1):
    for sy in (1, -1):
        lid = lid.union(
            cq.Workplane("XY", origin=(sx * POST_C[0], sy * POST_C[1], PCB_T))
            .box(POST, POST, CAV_D - PCB_T, centered=(True, True, False))
        )
# snap bumps on the lip
for sx in (1, -1):
    for sy in (1, -1):
        lid = lid.union(
            cq.Workplane("XY")
            .sphere(0.6)
            .translate((sx * (CAV_W / 2 - LIP_CLR), sy * SNAP_Y, SNAP_Z))
        )
# the bays are taller than the tray, so the lid roofs them
for sign in (1, -1):
    lid = lid.cut(cq.Workplane("XY").newObject([port_cutter(sign)]), clean=False)
lid = lid.edges(">Z").fillet(TOP_FILLET)

# --- lid graphics ---
# Everything reads across the width. The LEDs and the BOOT button all sit on
# the +x side of the board, so their labels are right-aligned into the empty
# column beside them instead of being stacked along the length, and the
# wordmark gets the clear band between the LEDs and the HOST end.
#
# All of it is built in absolute coordinates and cut in one pass. Re-selecting
# faces(">Z") between engravings does not work: the islands inside letter
# counters are coplanar with the lid top, so the selector starts matching them
# and every later mark lands relative to whichever letter it picked.
ARROW_W = 6.0
ARROW_BASE = 36.1
ARROW_TIP = 38.6
PORT_LABEL_Y = 31.8
PORT_LABEL_SIZE = 4.5
WORDMARK_Y = 17.8
WORDMARK_SIZE = 4.4
PIN_LABEL_SIZE = 2.6
PIN_LABEL_RIGHT = 3.0  # right edge of STAT / PWR / BOOT

TOP_Z = CAV_D + LID_T


def engrave(label, x, y, size, halign="center"):
    return (
        cq.Workplane("XY", origin=(x, y, TOP_Z - TEXT_DEPTH))
        .text(label, size, TEXT_DEPTH, kind="bold", halign=halign)
    )


def arrow(sign):
    """Solid triangle pointing at the port on the *sign end."""
    pts = [(-ARROW_W / 2, sign * ARROW_BASE), (ARROW_W / 2, sign * ARROW_BASE),
           (0.0, sign * ARROW_TIP)]
    return (
        cq.Workplane("XY", origin=(0, 0, TOP_Z - TEXT_DEPTH))
        .polyline(pts)
        .close()
        .extrude(TEXT_DEPTH)
    )


def thru_hole(x, y, d):
    return (
        cq.Workplane("XY", origin=(x, y, CAV_D - 1))
        .circle(d / 2)
        .extrude(LID_T + 2)
    )


# The graphics are kept separate from the holes: they are both the pockets
# cut into the lid and, exported on their own, the white inlay that drops
# back into them.
inlay = engrave("restorekit", 0, WORDMARK_Y, WORDMARK_SIZE)
for m in (
    engrave("HOST", 0, PORT_LABEL_Y, PORT_LABEL_SIZE),
    engrave("TARGET", 0, -PORT_LABEL_Y, PORT_LABEL_SIZE),
    arrow(1),
    arrow(-1),
    engrave("STAT", PIN_LABEL_RIGHT, LED2[1], PIN_LABEL_SIZE, halign="right"),
    engrave("PWR", PIN_LABEL_RIGHT, LED1[1], PIN_LABEL_SIZE, halign="right"),
    engrave("BOOT", PIN_LABEL_RIGHT, SW1[1], PIN_LABEL_SIZE, halign="right"),
):
    inlay = inlay.union(m)

marks = inlay
for h in (
    thru_hole(*LED1, 1.8),   # PWR window
    thru_hole(*LED2, 1.8),   # STAT window
    thru_hole(*SW1, 2.4),    # BOOT, paperclip
):
    marks = marks.union(h)
lid = lid.cut(marks, clean=False)

# --- export ---
out = os.path.join(os.path.dirname(__file__), "output")
os.makedirs(out, exist_ok=True)

assembly = (
    cq.Assembly()
    .add(bottom, name="bottom", color=cq.Color(0.25, 0.25, 0.28))
    .add(lid, name="lid", color=cq.Color(0.85, 0.85, 0.87))
    .add(inlay, name="inlay", color=cq.Color(1.0, 1.0, 1.0))
)
assembly.export(os.path.join(out, "dongle-lite-case.step"))

cq.exporters.export(bottom, os.path.join(out, "bottom.stl"))
# flip the lid so its flat top sits on the print bed. The inlay gets the same
# transform, so loading it as a second part lands it in its pockets with no
# repositioning.
flip = lambda w: w.rotate((0, 0, 0), (0, 1, 0), 180)
cq.exporters.export(flip(lid), os.path.join(out, "lid.stl"))
cq.exporters.export(flip(inlay), os.path.join(out, "lid-inlay.stl"))

print("wrote", out)
print(f"outer: {OUT_W} x {OUT_L} x {OUT_H} mm")
print(f"port bay: {PORT_W} x {PORT_Z1 - PORT_Z0} mm, wall open to the receptacle")
