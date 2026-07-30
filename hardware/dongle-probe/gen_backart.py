#!/usr/bin/env python3
"""Back silkscreen art: an EKG 'revival pulse' running the length of the
board, the restorekit wordmark, and a couple of medical crosses. Text is
mirrored so it reads correctly looking at the back of the board. Silk over
exposed copper is subtracted at plot time (--subtract-soldermask), and vias
are tented, so the print survives the annulars it crosses. KiCad python."""
import os

import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
BSILK = b.GetLayerID("B.SilkS")  # resolve before edits; bindings go stale


def mm(v):
    return pcbnew.FromMM(v)


def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))


def seg(x1, y1, x2, y2, w):
    s = pcbnew.PCB_SHAPE(b)
    s.SetShape(pcbnew.SHAPE_T_SEGMENT)
    s.SetStart(P(x1, y1))
    s.SetEnd(P(x2, y2))
    s.SetWidth(mm(w))
    s.SetLayer(BSILK)
    b.Add(s)


def arc(sx, sy, mx, my, ex, ey, w):
    s = pcbnew.PCB_SHAPE(b)
    s.SetShape(pcbnew.SHAPE_T_ARC)
    s.SetArcGeometry(P(sx, sy), P(mx, my), P(ex, ey))
    s.SetWidth(mm(w))
    s.SetLayer(BSILK)
    b.Add(s)


def dot(x, y, r):
    s = pcbnew.PCB_SHAPE(b)
    s.SetShape(pcbnew.SHAPE_T_CIRCLE)
    s.SetCenter(P(x, y))
    s.SetEnd(P(x + r, y))
    s.SetFilled(True)
    s.SetWidth(0)
    s.SetLayer(BSILK)
    b.Add(s)


def text(label, x, y, size, thick, angle=0):
    t = pcbnew.PCB_TEXT(b)
    t.SetText(label)
    t.SetPosition(P(x, y))
    t.SetTextSize(pcbnew.VECTOR2I(mm(size), mm(size)))
    t.SetTextThickness(mm(thick))
    t.SetTextAngleDegrees(angle)
    t.SetMirrored(True)
    t.SetLayer(BSILK)
    b.Add(t)


# border: rounded-rect echo of the outline, inset 1.5mm
BX0, BY0, BX1, BY1, BR = 101.5, 101.5, 114.5, 141.5, 1.2
BK = BR * (1 - 0.5**0.5)
BW = 0.25
seg(BX0 + BR, BY0, BX1 - BR, BY0, BW)
seg(BX1, BY0 + BR, BX1, BY1 - BR, BW)
seg(BX1 - BR, BY1, BX0 + BR, BY1, BW)
seg(BX0, BY1 - BR, BX0, BY0 + BR, BW)
arc(BX0, BY0 + BR, BX0 + BK, BY0 + BK, BX0 + BR, BY0, BW)
arc(BX1 - BR, BY0, BX1 - BK, BY0 + BK, BX1, BY0 + BR, BW)
arc(BX1, BY1 - BR, BX1 - BK, BY1 - BK, BX1 - BR, BY1, BW)
arc(BX0 + BR, BY1, BX0 + BK, BY1 - BK, BX0, BY1 - BR, BW)

# EKG trace down the middle: flatline, P bump, Q-R-S spike, T bump, flatline
EW = 0.55
PULSE = [
    (108.0, 106.0),
    (108.0, 112.5),   # flatline
    (109.2, 113.6),   # P
    (108.0, 114.7),
    (108.0, 116.2),
    (106.8, 117.3),   # Q
    (113.5, 119.0),   # R
    (105.2, 120.7),   # S
    (108.0, 121.9),
    (108.0, 124.5),
    (110.3, 126.2),   # T
    (108.0, 127.9),
    (108.0, 133.0),   # flatline out
]
for a, c in zip(PULSE, PULSE[1:]):
    seg(a[0], a[1], c[0], c[1], EW)
dot(108.0, 106.0, 0.55)   # pulse origin
dot(108.0, 133.0, 0.55)

# medical cross, opposite the wordmark
seg(112.0, 130.7, 113.6, 130.7, 0.35)
seg(112.8, 129.9, 112.8, 131.5, 0.35)

# wordmark + model
text("restorekit", 103.2, 119.0, 2.4, 0.38, angle=90)
text("dongle-probe", 108.0, 134.6, 1.25, 0.2)

pcbnew.SaveBoard(BOARD, b)
print("back silk art drawn")
os._exit(0)  # skip teardown; the stale swig runtime segfaults in atexit
