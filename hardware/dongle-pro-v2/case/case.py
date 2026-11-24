"""Dongle-Pro v2 enclosure, generated with CadQuery.

Two pieces, no screws, no supports: a tray the board drops into and a snap
lid with a printed-in BOOT button and LED windows. Dimensions come from
dongle-pro-v2.kicad_pcb (21 x 48 mm, board center at kicad (110.5, 124)).

Run:    uv run case.py            STEP + STLs into ./output/
Check:  uv run check.py           fit checks against the board STEP
"""

import os

import cadquery as cq

# --- PCB facts (from dongle-pro-v2.kicad_pcb) ---
PCB_W = 21.0
PCB_L = 48.0
PCB_T = 1.6
KX0, KY0 = 110.5, 124.0  # board center in kicad coords


def k(kx, ky):
    """kicad -> case coords. +y is the HOST end, z = 0 at the PCB bottom."""
    return (kx - KX0, KY0 - ky)


SW1 = k(117.4, 133.6)   # BOOT, TS3315A, top at z 3.10
D1 = k(120.2, 138.3)    # PWR LED
D2 = k(119.3, 130.9)    # STAT LED
SW1_TOP = 3.10
# HX 24P QCHT receptacles. The 3D model in the lib sits 1.6 mm too low (it
# puts the shell through a board that has no cutout), so these are the
# model's numbers + 1.6. At the mouth the shell is 8.98 wide, z 1.61..4.78,
# and sticks 0.53 past the board edge.
USB_W = 8.98
USB_Z0, USB_Z1 = 1.61, 4.78
USB_Z = (USB_Z0 + USB_Z1) / 2
USB_OUT = 0.53

# --- case parameters ---
CLR = 0.3       # PCB clearance per side
END_CLR = 0.15  # at the port ends; JLC holds the outline to about +/-0.1
WALL = 1.8
# The end walls wrap the bit of each receptacle that sticks out past the
# board, so the mouth sits 0.42 mm behind the face: about what a phone case
# does, and plugs still seat. A thicker end wall pushes the face out and the
# overmold hits it before the plug is home (the Lite's first case did that).
END_WALL = 0.8
FLOOR = 1.6
LID_T = 1.6
HEADROOM = 4.0  # cavity above the PCB top (receptacle top is 3.18 above it)

CAV_W = PCB_W + 2 * CLR
CAV_L = PCB_L + 2 * END_CLR
CAV_D = PCB_T + HEADROOM  # z of the seam
OUT_W = CAV_W + 2 * WALL
OUT_L = CAV_L + 2 * END_WALL
TOP_Z = CAV_D + LID_T
CORNER_R = 2.0  # keeps 0.8 mm of wall at the corners
EDGE_R = 0.6

# Stadium-shaped hole hugging the shell. The tray's end wall is notched open
# above the hole's centre so nothing bridges; a tongue on the lid fills the
# top of the notch.
PORT_CLR = 0.25
PORT_W = USB_W + 2 * PORT_CLR
PORT_H = USB_Z1 - USB_Z0 + 2 * PORT_CLR
TONGUE_Z0 = USB_Z + 1.2  # below this the tongue would be a sliver

LIP_T = 1.2
LIP_H = 2.2
LIP_CLR = 0.15
LIP_IN = CAV_W / 2 - LIP_CLR - LIP_T  # inner face of the lip, x

# Blocks on the lid in the four end corners: they pin the PCB to the floor
# and back up the thin end walls. Inner x edge per corner (x sign, y sign):
# clear of the receptacle legs (+/-4.82) and, at TARGET +x, of the TP11 pad.
BULK_D = 2.0
BULK_X = {(1, 1): 5.4, (-1, 1): 5.4, (1, -1): 8.7, (-1, -1): 5.4}

# Snap bumps want a solid run of lip either side of them: next to a cut the
# lip end just flexes away and there's no click. Keep them clear of the key
# gap (y 14.7..17.3), the PWR window (y -14.3) and the pry notch (|y| < 3).
SNAP_Y = 8.0
# Each snap is an 8 mm ridge on the lid lip into a groove in the tray wall. Round
# bumps were a single point of contact and the lid closed without much of a
# click. The groove sits low enough to leave 0.6 mm of wall above it as the
# catch, and the ridge passes 0.45 mm into the wall, same as the bumps did.
SNAP_L = 8.0
SNAP_Z = CAV_D - 1.5
RIDGE_R = 0.8
RIDGE_X = CAV_W / 2 - LIP_CLR - 0.2  # axis, inside the lip face
GROOVE_R = 0.9

# Key: a rib on the +x wall at the HOST half and a matching gap in the lid
# lip. Put the lid on backwards and the rib holds it 1.6 mm proud.
KEY_Y = 16.0
KEY_W = 2.0
KEY_D = 0.8
KEY_Z0 = 3.0  # clear of every part on that edge

PRY_W = 6.0  # fingernail notch at the seam on the -x side

# BOOT flexure: a tab cut free on three sides, hinged toward -x, lying in the
# gap between the two LEDs. A nub under it sits 0.25 mm over SW1.
TAB_X0 = -1.6        # hinge line
TAB_X1 = 8.1         # free end
TAB_Y = (-12.4, -9.0)
TAB_T = 1.0          # thinned from the inside
SLOT = 0.6
NUB = (SW1[0], -10.2)
NUB_R = 0.9
NUB_GAP = 0.25

LED_R = 0.75
# D1 sits under the lip, so its window would cut it in two. A collar around
# the window keeps the lip in one piece.
COLLAR_R = LED_R + 0.9
TEXT_DEPTH = 0.6     # 3 x 0.2 mm layers, the lid prints top-face-down
RING_DEPTH = 0.4


def box(x0, x1, y0, y1, z0, z1):
    return cq.Workplane("XY").box(
        x1 - x0, y1 - y0, z1 - z0, centered=False
    ).translate((x0, y0, z0))


def cyl(x, y, r, z0, z1):
    return cq.Workplane("XY", origin=(x, y, z0)).circle(r).extrude(z1 - z0)


def outer_profile(z0, z1, face):
    """Rounded block with its outside face (">Z" or "<Z") edge-filleted up
    front: filleting after the cuts trips OCC's clean()."""
    return (
        cq.Workplane("XY", origin=(0, 0, z0))
        .box(OUT_W, OUT_L, z1 - z0, centered=(True, True, False))
        .edges("|Z")
        .fillet(CORNER_R)
        .faces(face)
        .edges()
        .fillet(EDGE_R)
    )


def snap_rod(x, sy, r, extra):
    """A rod along y at one snap: the lid's ridge or (with extra length) the groove."""
    y0 = sy * SNAP_Y - SNAP_L / 2 - extra
    rod = cq.Solid.makeCylinder(r, SNAP_L + 2 * extra, cq.Vector(x, y0, SNAP_Z), cq.Vector(0, 1, 0))
    return cq.Workplane("XY").newObject([rod])


def port_hole(sign):
    """The stadium through one end wall, drawn in XY and stood up on X."""
    depth = 2 * END_WALL + 2
    hole = (cq.Workplane("XY", origin=(0, 0, -depth / 2))
            .slot2D(PORT_W, PORT_H).extrude(depth).val()
            .rotate((0, 0, 0), (1, 0, 0), 90)
            .translate((0, sign * OUT_L / 2, USB_Z)))
    return cq.Workplane("XY").newObject([hole])


def end_wall(sign, x, z0, z1):
    y0, y1 = sorted((sign * (CAV_L / 2 - 0.5), sign * (OUT_L / 2 + 1)))
    return box(-x, x, y0, y1, z0, z1)


def port_cutter(sign):
    """What the tray loses at one end: the hole plus the notch above it."""
    return port_hole(sign).union(
        end_wall(sign, PORT_W / 2, USB_Z, CAV_D + 1), clean=False)


def text(label, x, y, size, depth=TEXT_DEPTH, halign="center"):
    return cq.Workplane("XY", origin=(x, y, TOP_Z - depth)).text(
        label, size, depth, kind="bold", halign=halign
    )


def arrow(sign, base, tip, w=6.0):
    pts = [(-w / 2, sign * base), (w / 2, sign * base), (0.0, sign * tip)]
    return (
        cq.Workplane("XY", origin=(0, 0, TOP_Z - TEXT_DEPTH))
        .polyline(pts).close().extrude(TEXT_DEPTH)
    )


def build():
    # --- tray ---
    cut = box(-CAV_W / 2, CAV_W / 2, -CAV_L / 2, CAV_L / 2, 0, CAV_D + 1)
    for s in (1, -1):
        cut = cut.union(port_cutter(s), clean=False)
    for sx in (1, -1):
        for sy in (1, -1):
            cut = cut.union(snap_rod(sx * CAV_W / 2, sy, GROOVE_R, 0.3), clean=False)
    cut = cut.union(box(-OUT_W / 2 - 1, -CAV_W / 2 - 1.0, -PRY_W / 2, PRY_W / 2,
                        CAV_D - 0.8, CAV_D + 1), clean=False)
    tray = outer_profile(-FLOOR, CAV_D, "<Z").cut(cut, clean=False)
    tray = tray.union(box(CAV_W / 2 - KEY_D, CAV_W / 2, KEY_Y - KEY_W / 2,
                          KEY_Y + KEY_W / 2, KEY_Z0, CAV_D))

    # --- lid ---
    lid = outer_profile(CAV_D, TOP_Z, ">Z")
    lip = (
        cq.Workplane("XY", origin=(0, 0, CAV_D - LIP_H))
        .rect(CAV_W - 2 * LIP_CLR, CAV_L - 2 * LIP_CLR)
        .rect(2 * LIP_IN, CAV_L - 2 * LIP_CLR - 2 * LIP_T)
        .extrude(LIP_H)
    )
    for s in (1, -1):  # the lip would sit on the receptacle
        lip = lip.cut(box(-5.1, 5.1, s * (CAV_L / 2 - 1) - 2,
                          s * (CAV_L / 2 - 1) + 2, 0, CAV_D))
    lip = lip.cut(box(LIP_IN - 0.1, CAV_W / 2, KEY_Y - KEY_W / 2 - 0.3,
                      KEY_Y + KEY_W / 2 + 0.3, 0, CAV_D))
    lid = lid.union(lip)
    collar = cyl(*D1, COLLAR_R, CAV_D - LIP_H, CAV_D).intersect(
        box(-CAV_W / 2, CAV_W / 2 - LIP_CLR, -CAV_L / 2, CAV_L / 2, 0, CAV_D))
    lid = lid.union(collar)
    for sx in (1, -1):
        for sy in (1, -1):
            x0, x1 = sorted((sx * BULK_X[sx, sy], sx * (CAV_W / 2 - LIP_CLR)))
            y0, y1 = sorted((sy * (CAV_L / 2 - BULK_D), sy * (CAV_L / 2 - LIP_CLR)))
            lid = lid.union(box(x0, x1, y0, y1, PCB_T, CAV_D))
            lid = lid.union(snap_rod(sx * RIDGE_X, sy, RIDGE_R, 0))
    for s in (1, -1):
        y0, y1 = sorted((s * CAV_L / 2, s * OUT_L / 2))
        tongue = box(-PORT_W / 2 + 0.1, PORT_W / 2 - 0.1, y0, y1, TONGUE_Z0, CAV_D + 0.01)
        lid = lid.union(tongue.cut(port_hole(s)))

    # BOOT flexure: U-slot through the lid, thin the tab, hang the nub off it
    ty0, ty1 = TAB_Y
    slot = box(TAB_X0, TAB_X1 + SLOT, ty0 - SLOT, ty1 + SLOT, CAV_D - 1, TOP_Z + 1)
    slot = slot.cut(box(TAB_X0 - 1, TAB_X1, ty0, ty1, CAV_D - 1, TOP_Z + 1))
    lid = lid.cut(slot, clean=False)
    lid = lid.cut(box(TAB_X0, TAB_X1, ty0, ty1, CAV_D - 1, TOP_Z - TAB_T),
                  clean=False)
    lid = lid.union(cyl(*NUB, NUB_R, SW1_TOP + NUB_GAP, TOP_Z - TAB_T + 0.01))

    # --- graphics ---
    # Built in absolute coords and cut in one pass: re-selecting faces(">Z")
    # between engravings lands later marks on letter counters (Lite lesson).
    # The LED windows go right through; the inlay fills their top 0.6 mm, so a
    # white inlay is a diffuser and no inlay is an open window.
    marks = [
        text("restorekit", 0, 7.0, 4.4),
        text("PRO", 0, 1.6, 3.0),
        text("HOST", 0, 17.6, 4.5),
        text("TARGET", 0, -17.8, 4.5),
        arrow(1, 20.3, 22.7),
        arrow(-1, 20.3, 22.7),
        text("BOOT", TAB_X0 - 1.0, sum(TAB_Y) / 2, 2.6, halign="right"),
        text("STAT", D2[0] - 1.2, D2[1], 2.6, halign="right"),
        text("PWR", D1[0] - 1.2, D1[1] - 0.1, 2.6, halign="right"),  # clear of the BOOT slot
    ]
    ring = (cq.Workplane("XY", origin=(*NUB, TOP_Z - RING_DEPTH))
            .circle(1.3).circle(0.8).extrude(RING_DEPTH))
    leds = [cyl(x, y, LED_R, TOP_Z - TEXT_DEPTH, TOP_Z) for x, y in (D1, D2)]
    inlay = marks[0]
    for m in marks[1:] + [ring] + leds:
        inlay = inlay.union(m)
    windows = cyl(*D1, LED_R, CAV_D - LIP_H - 1, TOP_Z + 1).union(
        cyl(*D2, LED_R, CAV_D - LIP_H - 1, TOP_Z + 1))
    lid = lid.cut(inlay.union(windows), clean=False)

    return {"tray": tray, "lid": lid, "inlay": inlay, "marks": marks,
            "slot": slot, "windows": windows}


def export(parts):
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "output")
    os.makedirs(out, exist_ok=True)
    (
        cq.Assembly()
        .add(parts["tray"], name="tray", color=cq.Color(0.16, 0.16, 0.18))
        .add(parts["lid"], name="lid", color=cq.Color(0.16, 0.16, 0.18))
        .add(parts["inlay"], name="inlay", color=cq.Color(1.0, 1.0, 1.0))
        .export(os.path.join(out, "dongle-pro-v2-case.step"))
    )
    cq.exporters.export(parts["tray"], os.path.join(out, "tray.stl"))
    # lid and inlay flipped together so the inlay drops into its pockets
    flip = lambda w: w.rotate((0, 0, 0), (0, 1, 0), 180)
    cq.exporters.export(flip(parts["lid"]), os.path.join(out, "lid.stl"))
    cq.exporters.export(flip(parts["inlay"]), os.path.join(out, "lid-inlay.stl"))
    return out


if __name__ == "__main__":
    out = export(build())
    print("wrote", out)
    print(f"outer: {OUT_W:.1f} x {OUT_L:.1f} x {TOP_Z + FLOOR:.1f} mm")
