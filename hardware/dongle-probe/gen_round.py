#!/usr/bin/env python3
"""Round the board outline corners: replace the square 16 x 38.4 mm rectangle
with edge segments + 2.0 mm corner arcs, then refill zones. The old square
outline carried the F.Cu GND pour around the bottom-right corner; rounding
cuts that sliver and strands the pour's bottom region, so a stitch via at
(102.0, 137.0) ties it down to the B.Cu plane. KiCad python.

Note: the swig bindings go stale once the board is edited (ZONE_FILLER and
even a second LoadBoard crash), so the refill runs in a subprocess."""
import os
import subprocess
import sys

import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
# swig bindings go stale once the board is edited; resolve the net up front
GND = b.GetNetsByName()["GND"]

X0, Y0, X1, Y1 = 100.0, 100.0, 116.0, 138.4
R = 2.0
K = R * (1 - 0.5**0.5)  # arc midpoint inset from the corner


def P(x, y):
    return pcbnew.VECTOR2I(pcbnew.FromMM(x), pcbnew.FromMM(y))


for d in list(b.GetDrawings()):
    if d.GetLayerName() == "Edge.Cuts":
        b.Remove(d)


def add(s):
    s.SetLayer(pcbnew.Edge_Cuts)
    s.SetWidth(pcbnew.FromMM(0.1))
    b.Add(s)


def seg(x1, y1, x2, y2):
    s = pcbnew.PCB_SHAPE(b)
    s.SetShape(pcbnew.SHAPE_T_SEGMENT)
    s.SetStart(P(x1, y1))
    s.SetEnd(P(x2, y2))
    add(s)


def arc(sx, sy, mx, my, ex, ey):
    s = pcbnew.PCB_SHAPE(b)
    s.SetShape(pcbnew.SHAPE_T_ARC)
    s.SetArcGeometry(P(sx, sy), P(mx, my), P(ex, ey))
    add(s)


seg(X0 + R, Y0, X1 - R, Y0)  # top
seg(X1, Y0 + R, X1, Y1 - R)  # right
seg(X1 - R, Y1, X0 + R, Y1)  # bottom
seg(X0, Y1 - R, X0, Y0 + R)  # left
arc(X0, Y0 + R, X0 + K, Y0 + K, X0 + R, Y0)
arc(X1 - R, Y0, X1 - K, Y0 + K, X1, Y0 + R)
arc(X1, Y1 - R, X1 - K, Y1 - K, X1 - R, Y1)
arc(X0 + R, Y1, X0 + K, Y1 - K, X0, Y1 - R)

v = pcbnew.PCB_VIA(b)
v.SetPosition(P(102.0, 137.0))
v.SetWidth(pcbnew.FromMM(0.6))
v.SetDrill(pcbnew.FromMM(0.3))
v.SetNet(GND)
v.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
b.Add(v)

pcbnew.SaveBoard(BOARD, b)
print("outline rounded r =", R, "; GND stitch via added")

REFILL = f"""
import pcbnew
b = pcbnew.LoadBoard({BOARD!r})
pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard({BOARD!r}, b)
print("zones refilled")
"""
subprocess.run([sys.executable, "-c", REFILL], check=True)
os._exit(0)  # skip teardown; the stale swig runtime segfaults in atexit
