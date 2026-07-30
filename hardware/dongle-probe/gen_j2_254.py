#!/usr/bin/env python3
"""Swap J2 from the 1.27mm Minitek127 to a 2.54mm 2x3 shrouded IDC box header
(BOOMELE 2.54-2*3P, LCSC C11214) so the TC2030-IDC-NL cable's 0.1" IDC socket
actually mates with it.

The DC3-style housing is 15.28 x 8.90 mm - far bigger than the Minitek - so
the board grows from 38.4 to 43.0 mm and J2 slides down into the new strip;
everything else stays put except JP1, which moves out from under the shroud
to the pocket west of D2. TGT_* routes are ripped and redrawn to the new
pads. Pad grid 2.54mm, drill 1.0, pad 1.7 (KiCad IDC-Header_2x03 geometry),
key notch on the odd-pin row facing the board edge, pin 1 west - same 1:1
TC2030 mapping as before.

KiCad python. Refill runs in a subprocess (see gen_round.py for why)."""
import os
import subprocess
import sys

import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
# resolve EVERY lookup before any edit; the swig bindings go stale after the
# first mutation and b.<anything>() starts returning unwrapped SwigPyObjects
NETS = b.GetNetsByName()
ALL_DRAWINGS = list(b.GetDrawings())
ALL_TRACKS = list(b.GetTracks())
ZONE_OUTLINES = [z.Outline().Outline(0) for z in b.Zones()]
fps = {fp.GetReference(): fp for fp in b.GetFootprints()}
J2_PADS = list(fps["J2"].Pads())
J2_FIELDS = fps["J2"].GetFieldsText()
J2_GRAPHICS = list(fps["J2"].GraphicalItems())
JP1_PADS = list(fps["JP1"].Pads())
JP1_GRAPHICS = list(fps["JP1"].GraphicalItems())
D2_PADS = list(fps["D2"].Pads())
R10_PADS = list(fps["R10"].Pads())

X0, Y0, X1 = 100.0, 100.0, 116.0
Y1 = 143.0  # was 138.4
R = 2.0
K = R * (1 - 0.5**0.5)

CX = 108.0            # pad-field center x
ODD_Y = 139.82        # pins 1/3/5, key-notch row, toward the board edge
EVEN_Y = 137.28       # pins 2/4/6
COLS = {1: 105.46, 3: 108.0, 5: 110.54}
BODY = (100.36, 134.1, 115.64, 143.0)   # 15.28 x 8.90, flush at the edge
SLOT_W, SLOT_D = 4.1, 1.2


def mm(v):
    return pcbnew.FromMM(v)


def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))


# --- stretch both GND zone outlines to the new rect (before any Remove) ---
for o in ZONE_OUTLINES:
    for i in range(o.PointCount()):
        if o.CPoint(i).y > mm(120):
            o.SetPoint(i, pcbnew.VECTOR2I(o.CPoint(i).x, mm(Y1)))

# --- outline: same rounded rect, 4.6mm longer ---
for d in ALL_DRAWINGS:
    if d.GetLayerName() == "Edge.Cuts":
        b.Remove(d)


def edge(s):
    s.SetLayer(pcbnew.Edge_Cuts)
    s.SetWidth(mm(0.1))
    b.Add(s)


def eseg(x1, y1, x2, y2):
    s = pcbnew.PCB_SHAPE(b)
    s.SetShape(pcbnew.SHAPE_T_SEGMENT)
    s.SetStart(P(x1, y1))
    s.SetEnd(P(x2, y2))
    edge(s)


def earc(sx, sy, mx, my, ex, ey):
    s = pcbnew.PCB_SHAPE(b)
    s.SetShape(pcbnew.SHAPE_T_ARC)
    s.SetArcGeometry(P(sx, sy), P(mx, my), P(ex, ey))
    edge(s)


eseg(X0 + R, Y0, X1 - R, Y0)
eseg(X1, Y0 + R, X1, Y1 - R)
eseg(X1 - R, Y1, X0 + R, Y1)
eseg(X0, Y1 - R, X0, Y0 + R)
earc(X0, Y0 + R, X0 + K, Y0 + K, X0 + R, Y0)
earc(X1 - R, Y0, X1 - K, Y0 + K, X1, Y0 + R)
earc(X1, Y1 - R, X1 - K, Y1 - K, X1 - R, Y1)
earc(X0 + R, Y1, X0 + K, Y1 - K, X0, Y1 - R)

# --- rip the old J2 stub routing (TGT_* nets are J2-only), the +3V3 stub
# that fed JP1 at its old spot, and the local LED tracks around D2/R10
# (that column shifts 0.3mm east to clear the relocated JP1) ---
for t in ALL_TRACKS:
    net = t.GetNetname()
    if net.startswith("TGT_") or net == "LEDA_A":
        b.Remove(t)
        continue
    if t.Type() == pcbnew.PCB_VIA_T:
        continue
    s, e = t.GetStart(), t.GetEnd()
    pts = {(round(s.x / 1e6, 2), round(s.y / 1e6, 2)),
           (round(e.x / 1e6, 2), round(e.y / 1e6, 2))}
    if net == "+3V3" and (112.65, 135.26) in pts:
        b.Remove(t)
    elif net == "LED_ACT" and (114.2, 128.81) in pts:
        b.Remove(t)

# --- J2 rework ---
j2 = fps["J2"]
j2.SetPosition(P(CX, (ODD_Y + EVEN_Y) / 2))
for k, v in (("Mpn", "2.54-2*3P"), ("MPN", "2.54-2*3P"),
             ("LCSC", "C11214"), ("Manufacturer", "BOOMELE"),
             ("Description", "2.54mm 2x3 shrouded keyed IDC box header"),
             ("Datasheet", "")):
    if k in J2_FIELDS:
        j2.SetField(k, v)
j2.SetValue("2.54-2*3P")

for pad in J2_PADS:
    n = int(pad.GetNumber())
    col = COLS[n if n % 2 else n - 1]
    row = ODD_Y if n % 2 else EVEN_Y
    pad.SetPosition(P(col, row))
    pad.SetDrillSize(pcbnew.VECTOR2I(mm(1.0), mm(1.0)))
    pad.SetSize(pcbnew.F_Cu, pcbnew.VECTOR2I(mm(1.7), mm(1.7)))

for g in J2_GRAPHICS:
    if g.GetClass() in ("PCB_SHAPE", "MGRAPHIC") and g.GetLayerName() in (
            "F.SilkS", "F.Silkscreen", "F.Fab", "F.CrtYd", "F.Courtyard"):
        j2.Remove(g)

FAB = b.GetLayerID("F.Fab")
SILK = b.GetLayerID("F.SilkS")
CRT = b.GetLayerID("F.CrtYd")


def fpseg(layer, w, ax, ay, bx, by):
    s = pcbnew.PCB_SHAPE(j2)
    s.SetShape(pcbnew.SHAPE_T_SEGMENT)
    s.SetStart(P(ax, ay))
    s.SetEnd(P(bx, by))
    s.SetWidth(mm(w))
    s.SetLayer(layer)
    j2.Add(s)


bx0, by0, bx1, by1 = BODY
sx0, sx1 = CX - SLOT_W / 2, CX + SLOT_W / 2
for lay, w in ((FAB, 0.1), (SILK, 0.12)):
    fpseg(lay, w, bx0, by0, bx1, by0)                # north wall
    fpseg(lay, w, bx0, by1, sx0, by1)                # south wall, west of slot
    fpseg(lay, w, sx1, by1, bx1, by1)
    fpseg(lay, w, bx0, by0, bx0, by1)
    fpseg(lay, w, bx1, by0, bx1, by1)
    fpseg(lay, w, sx0, by1, sx0, by1 - SLOT_D)       # key slot
    fpseg(lay, w, sx1, by1, sx1, by1 - SLOT_D)
    fpseg(lay, w, sx0, by1 - SLOT_D, sx1, by1 - SLOT_D)
fpseg(SILK, 0.25, 104.0, 140.9, 104.0, 141.7)        # pin-1 tick
c = 0.25
for a, bp in (((bx0 - c, by0 - c), (bx1 + c, by0 - c)),
              ((bx1 + c, by0 - c), (bx1 + c, by1 + c)),
              ((bx1 + c, by1 + c), (bx0 - c, by1 + c)),
              ((bx0 - c, by1 + c), (bx0 - c, by0 - c))):
    fpseg(CRT, 0.05, a[0], a[1], bp[0], bp[1])

# --- JP1 out from under the shroud, rotated, west of D2 ---
jp1 = fps["JP1"]
jp1.SetOrientationDegrees(90)
jp1.SetPosition(P(112.75, 131.2))
for pad in JP1_PADS:
    pad.SetPosition(P(112.75, 130.55 if pad.GetNumber() == "1" else 131.85))
for g in JP1_GRAPHICS:
    if g.GetClass() in ("PCB_SHAPE", "MGRAPHIC") and g.GetLayerName() in (
            "F.SilkS", "F.Silkscreen", "F.CrtYd", "F.Courtyard"):
        jp1.Remove(g)
# tight courtyard (the stock one is 2.55mm wide and can't fit between
# SW1's pads and D2's courtyard)
jx0, jy0, jx1, jy1 = 111.9, 129.95, 113.6, 132.45


def jpseg(ax, ay, bx, by):
    s = pcbnew.PCB_SHAPE(jp1)
    s.SetShape(pcbnew.SHAPE_T_SEGMENT)
    s.SetStart(P(ax, ay))
    s.SetEnd(P(bx, by))
    s.SetWidth(mm(0.05))
    s.SetLayer(CRT)
    jp1.Add(s)


jpseg(jx0, jy0, jx1, jy0)
jpseg(jx1, jy0, jx1, jy1)
jpseg(jx1, jy1, jx0, jy1)
jpseg(jx0, jy1, jx0, jy0)

# --- D2 + R10 shift 0.3mm east so their courtyards clear JP1's ---
fps["D2"].SetPosition(P(114.5, 130.72))
for pad in D2_PADS:
    pad.SetPosition(P(114.5, 129.97 if pad.GetNumber() == "1" else 131.47))
fps["R10"].SetPosition(P(114.5, 128.30))
for pad in R10_PADS:
    pad.SetPosition(P(114.5, 128.81 if pad.GetNumber() == "1" else 127.79))

# --- redraw the stub routes ---
def track(net, layer, pts, w=0.16):
    for a, bp in zip(pts, pts[1:]):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(P(*a))
        t.SetEnd(P(*bp))
        t.SetWidth(mm(w))
        t.SetLayer(layer)
        t.SetNet(NETS[net])
        b.Add(t)


def via(net, x, y):
    v = pcbnew.PCB_VIA(b)
    v.SetPosition(P(x, y))
    v.SetWidth(mm(0.6))
    v.SetDrill(mm(0.3))
    v.SetNet(NETS[net])
    v.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
    b.Add(v)


F, B = pcbnew.F_Cu, pcbnew.B_Cu
track("+3V3", F, [(112.65, 126.28), (112.65, 130.4), (112.75, 130.55)])
track("LED_ACT", F, [(115.7046, 127.3005), (115.7046, 128.8051), (114.5, 128.8051)])
track("LEDA_A", F, [(114.5, 127.79), (113.92, 128.37), (113.92, 129.39),
                    (114.5, 129.97)])
track("TGT_VTREF", F, [(112.75, 131.85), (112.75, 133.3)])
via("TGT_VTREF", 112.75, 133.3)
track("TGT_VTREF", B, [(112.75, 133.3), (112.75, 141.5), (105.46, 141.5),
                       (105.46, 139.82)])
track("TGT_SWDIO", F, [(106.31, 132.73), (106.31, 136.13), (105.46, 136.98),
                       (105.46, 137.28)])
track("TGT_SWCLK", F, [(104.21, 132.73), (104.21, 138.55), (107.2, 138.55),
                       (108.0, 137.75), (108.0, 137.28)])
track("TGT_RESET", F, [(108.41, 132.73), (108.41, 134.8), (109.27, 135.66),
                       (109.27, 138.9), (108.35, 139.82), (108.0, 139.82)])
track("TGT_BOOT", F, [(110.51, 132.73), (110.51, 136.9), (110.54, 137.28)])

pcbnew.SaveBoard(BOARD, b)
print("J2 swapped to 2.54mm box header; board 16 x", Y1 - Y0, "mm")

REFILL = f"""
import pcbnew
b = pcbnew.LoadBoard({BOARD!r})
pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard({BOARD!r}, b)
print("zones refilled")
"""
subprocess.run([sys.executable, "-c", REFILL], check=True)
os._exit(0)  # skip teardown; the stale swig runtime segfaults in atexit
