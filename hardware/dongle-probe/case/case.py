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
FLOOR = 1.6
LID_T = 1.6
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


def outer_profile(height, z0):
    return (
        cq.Workplane("XY", origin=(0, 0, z0))
        .box(OUT_W, OUT_L, height, centered=(True, True, False))
        .edges("|Z")
        .fillet(CORNER_R)
    )


# --- bottom tray ---
cutters = cq.Workplane("XY").box(CAV_W, CAV_L, CAV_D, centered=(True, True, False))
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
# LED windows and BOOT hole
lid = (
    lid.faces(">Z")
    .workplane(centerOption="CenterOfBoundBox")
    .pushPoints([LED_PWR, LED_ACT])
    .hole(1.8)
    .pushPoints([SW1])
    .hole(2.6)
)


def engrave(body, label, x, y, size):
    return (
        body.faces(">Z")
        .workplane(centerOption="CenterOfBoundBox")
        .transformed(offset=(x, y, 0), rotate=(0, 0, 90))
        .text(label, size, -TEXT_DEPTH, kind="bold")
    )


def text_len(label, size):
    """Measured length of the label along its reading direction."""
    t = cq.Workplane("XY").text(label, size, 1, kind="bold")
    return t.val().BoundingBox().xlen


ARROW_BASE = 18.5


def arrow(body):
    """Solid triangle pointing at the USB port."""
    pts = [(-2.5, ARROW_BASE), (2.5, ARROW_BASE), (0.0, ARROW_BASE + 2.5)]
    return (
        body.faces(">Z")
        .workplane(centerOption="CenterOfBoundBox")
        .polyline(pts)
        .close()
        .cutBlind(-TEXT_DEPTH)
    )


# wordmark centered, USB arrow at the +Y end, small labels beside their holes
lid = engrave(lid, "restorekit", 0, 4.0, 4.2)
lid = arrow(lid)
lid = engrave(lid, "PWR", LED_PWR[0], LED_PWR[1] + 5.1, 2.2)
lid = engrave(lid, "ACT", LED_ACT[0], LED_ACT[1] + 5.1, 2.2)
lid = engrave(lid, "BOOT", 2.9, SW1[1], 2.2)

# --- export ---
out = os.path.join(os.path.dirname(__file__), "output")
os.makedirs(out, exist_ok=True)

assembly = (
    cq.Assembly()
    .add(bottom, name="bottom", color=cq.Color(0.25, 0.25, 0.28))
    .add(lid, name="lid", color=cq.Color(0.85, 0.85, 0.87))
)
assembly.export(os.path.join(out, "dongle-probe-case.step"))

cq.exporters.export(bottom, os.path.join(out, "bottom.stl"))
# flip the lid so its flat top sits on the print bed
lid_print = lid.rotate((0, 0, 0), (0, 1, 0), 180)
cq.exporters.export(lid_print, os.path.join(out, "lid.stl"))

print("wrote", out)
print(f"outer: {OUT_W} x {OUT_L} x {FLOOR + CAV_D + LID_T} mm")
