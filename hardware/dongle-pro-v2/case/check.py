"""Fit checks for the Dongle-Pro v2 case against the board's own STEP.

uv run check.py   (exits 1 on any failure)
"""

import os
import sys

import cadquery as cq

import case as c

HERE = os.path.dirname(os.path.abspath(__file__))
STEP = os.path.join(HERE, "..", "mfg-jlcpcb", "dongle-pro-v2.step")
EPS = 1e-3
fails = []


def check(ok, msg):
    print(("ok   " if ok else "FAIL ") + msg)
    if not ok:
        fails.append(msg)


def vol(a, b):
    return a.intersect(b).Volume()


def to_case(s):
    # STEP is in kicad mm with y flipped; receptacle models sit 1.6 mm low
    b = s.BoundingBox()
    if 105.6 < b.xmin and b.xmax < 115.4 and b.zmax < 3.3 and (
            b.ymin > -109.4 or b.ymax < -138.6):
        s = s.translate(cq.Vector(0, 0, c.PCB_T))
    return s.translate(cq.Vector(-c.KX0, c.KY0, 0))


parts = c.build()
tray, lid = parts["tray"].val(), parts["lid"].val()
board = [to_case(s) for s in cq.importers.importStep(STEP).solids().vals()]
pcb = max(board, key=lambda s: s.Volume())
comps = cq.Compound.makeCompound([s for s in board if s is not pcb])

check(vol(tray, comps) < EPS, "tray clears every part")
check(vol(lid, comps) < EPS, "lid clears every part")
check(vol(tray, pcb) < EPS and vol(lid, pcb) < EPS, "PCB sits on the floor, posts on top")
check(vol(tray, lid) < 0.01, f"lid seats in tray (overlap {vol(tray, lid):.3f} mm3)")
backwards = lid.rotate(cq.Vector(), cq.Vector(0, 0, 1), 180)
check(vol(tray, backwards) > 0.5, "key stops a backwards lid")

top = max(s.BoundingBox().zmax for s in board)
print(f"     tallest part z {top:.2f}, lid underside z {c.CAV_D:.2f}")

sw1 = [s for s in board if s.BoundingBox().zmax > 3.0
       and abs(s.Center().x - c.SW1[0]) < 2 and abs(s.Center().y - c.SW1[1]) < 2]
gap = min(lid.distance(s) for s in sw1)
check(0.1 < gap < 0.4, f"BOOT nub {gap:.2f} mm over SW1")
# each snap bump needs unbroken lip 4 mm either side, or it flexes instead of clicking
for sx in (1, -1):
    for sy in (1, -1):
        x0, x1 = sorted((sx * c.LIP_IN, sx * (c.CAV_W / 2 - c.LIP_CLR)))
        run = cq.Solid.makeBox(x1 - x0, c.SNAP_L + 2, c.LIP_H,
                               cq.Vector(x0, sy * c.SNAP_Y - c.SNAP_L / 2 - 1, c.CAV_D - c.LIP_H))
        fill = vol(lid, run) / run.Volume()
        check(fill > 0.99, f"lip solid around the snap at ({sx:+d}, {sy * c.SNAP_Y:+.0f}): {fill:.0%}")
bite = c.RIDGE_X + c.RIDGE_R - c.CAV_W / 2
catch = c.CAV_D - (c.SNAP_Z + c.GROOVE_R)
check(0.35 < bite < 0.6 and catch >= 0.5, f"snap ridge bites {bite:.2f} mm, {catch:.2f} mm of wall above the groove")
strain = 1.5 * c.TAB_T * (gap + 0.25) / (c.NUB[0] - c.TAB_X0) ** 2
check(strain < 0.02, f"flexure strain {strain * 100:.1f}% at full press")

recess = c.OUT_L / 2 - (c.PCB_L / 2 + c.USB_OUT)
check(recess < 0.5, f"mouth {recess:.2f} mm behind the face")
# board slid to either end of its 0.15 mm play: receptacles still clear the holes
for dy in (c.END_CLR, -c.END_CLR):
    moved = comps.translate(cq.Vector(0, dy, 0))
    check(vol(tray, moved) < EPS and vol(lid, moved) < EPS, f"parts clear with the board shifted {dy:+.2f}")

# graphics stay off the slot, the windows, each other, the edge and the bays
marks = parts["marks"]
keep_out = [("slot", parts["slot"].val()), ("windows", parts["windows"].val())]
keep_out += [(f"bay{s}", c.port_cutter(s).val()) for s in (1, -1)]
prism = cq.Workplane("XY").box(c.OUT_W, c.OUT_L, 20).edges("|Z").fillet(c.CORNER_R)
edge = cq.Workplane("XY").box(c.OUT_W + 10, c.OUT_L + 10, 20).cut(prism).val()
for i, m in enumerate(marks):
    m = m.val()
    for name, ko in keep_out:
        d = m.distance(ko)
        if d < 0.35:
            check(False, f"mark {i} is {d:.2f} mm from {name}")
    for j, n in enumerate(marks[i + 1:], i + 1):
        d = m.distance(n.val())
        if d < 0.35:
            check(False, f"marks {i} and {j} are {d:.2f} mm apart")
    d = m.distance(edge)
    if d < c.EDGE_R + 0.3:
        check(False, f"mark {i} is {d:.2f} mm from the lid edge")
check(not any(f.startswith("mark") for f in fails), "graphics clear of each other and the cutouts")

sys.exit(1 if fails else 0)
