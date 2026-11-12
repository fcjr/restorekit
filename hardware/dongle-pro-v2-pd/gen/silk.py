#!/usr/bin/env python3
"""Silkscreen. Run with KiCad's python after gen/zones.py.

Front: the footprints' own silk is stripped (the board is too dense for
designators) and replaced with pin-1 dots and the few labels a person needs.
Back: no parts, so it is the artwork side. A flatline leaves the host port,
starts beating on its way down the board and runs through the restorekit mark
at the target end.
"""
import os, sys, math
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import pcbnew

PCB = 'dongle-pro-v2-pd.kicad_pcb'
board = pcbnew.LoadBoard(PCB)
F, B = pcbnew.F_SilkS, pcbnew.B_SilkS
MM = pcbnew.FromMM
V = pcbnew.VECTOR2I_MM
CX = 110.5

pad = {}
for fp in board.GetFootprints():
    for p in fp.Pads():
        pad[(fp.GetReference(), p.GetNumber())] = (p.GetPosition().x / 1e6, p.GetPosition().y / 1e6)
fps = {fp.GetReference(): fp for fp in board.GetFootprints()}

# ---- strip footprint silk, park designators on the fab layer.
# Field edits first, removals last: pcbnew's bindings go stale after a Remove.
doomed = []
for fp in board.GetFootprints():
    for fld in fp.GetFields():
        if fld.GetLayer() in (F, B):
            fld.SetLayer(pcbnew.F_Fab)
        fld.SetVisible(False)
    for g in fp.GraphicalItems():
        if g.GetLayer() in (F, B):
            doomed.append((fp, g))
for fp, g in doomed:
    g.SetLayer(pcbnew.F_Fab)

def text(s, x, y, layer, size, th=None, angle=0, w_ratio=0.9, italic=False):
    t = pcbnew.PCB_TEXT(board)
    t.SetText(s); t.SetPosition(V(x, y)); t.SetLayer(layer)
    t.SetTextSize(pcbnew.VECTOR2I(MM(size * w_ratio), MM(size)))
    t.SetTextThickness(MM(th if th else max(0.12, size * 0.15)))
    t.SetTextAngle(pcbnew.EDA_ANGLE(angle, pcbnew.DEGREES_T))
    t.SetHorizJustify(pcbnew.GR_TEXT_H_ALIGN_CENTER); t.SetVertJustify(pcbnew.GR_TEXT_V_ALIGN_CENTER)
    t.SetItalic(italic)
    if layer == B: t.SetMirrored(True)
    board.Add(t)

def seg(a, b, layer, w):
    s = pcbnew.PCB_SHAPE(board); s.SetShape(pcbnew.SHAPE_T_SEGMENT)
    s.SetStart(V(*a)); s.SetEnd(V(*b)); s.SetLayer(layer); s.SetWidth(MM(w)); board.Add(s)

def poly(pts, layer, w):
    for a, b in zip(pts, pts[1:]): seg(a, b, layer, w)

def arc(start, mid, end, layer, w):
    s = pcbnew.PCB_SHAPE(board); s.SetShape(pcbnew.SHAPE_T_ARC)
    s.SetArcGeometry(V(*start), V(*mid), V(*end)); s.SetLayer(layer); s.SetWidth(MM(w)); board.Add(s)

def dot(x, y, r, layer):
    s = pcbnew.PCB_SHAPE(board); s.SetShape(pcbnew.SHAPE_T_CIRCLE)
    s.SetStart(V(x, y)); s.SetEnd(V(x + r, y)); s.SetFilled(True); s.SetLayer(layer); s.SetWidth(0); board.Add(s)

def rrect(cx, cy, w, h, r, layer, lw):
    x0, x1, y0, y1 = cx - w / 2, cx + w / 2, cy - h / 2, cy + h / 2; k = r * (1 - math.sqrt(0.5))
    seg((x0 + r, y0), (x1 - r, y0), layer, lw); seg((x1, y0 + r), (x1, y1 - r), layer, lw)
    seg((x1 - r, y1), (x0 + r, y1), layer, lw); seg((x0, y1 - r), (x0, y0 + r), layer, lw)
    arc((x1 - r, y0), (x1 - k, y0 + k), (x1, y0 + r), layer, lw); arc((x1, y1 - r), (x1 - k, y1 - k), (x1 - r, y1), layer, lw)
    arc((x0 + r, y1), (x0 + k, y1 - k), (x0, y1 - r), layer, lw); arc((x0, y0 + r), (x0 + k, y0 + k), (x0 + r, y0), layer, lw)

def beat(bx, yc, a, sx=1):
    """One heartbeat along +y on baseline x=bx; spikes go +x (sx=-1 flips)."""
    f = lambda dx, dy: (bx + sx * dx * a, yc + dy)
    return [f(0, -1.9), f(0.12, -1.5), f(0, -1.1), f(0, -0.7), f(-0.18, -0.5), f(1.0, -0.1),
            f(-0.45, 0.35), f(0, 0.6), f(0, 1.0), f(0.22, 1.5), f(0, 2.0)]

# ======================================================================= back
BX = 113.2
MARK_Y, MARK = 131.5, 11.0
line = [(BX, 109.2)]
for yc, a in ((114.2, 0.9), (120.6, 2.0)):
    line += beat(BX, yc, a)
line += beat(BX, MARK_Y, 4.3) + [(BX, 138.0)]
poly(line, B, 0.32)
dot(BX, 109.2, 0.32, B); dot(BX, 138.0, 0.32, B)
# the mark: the chip outline with the pulse through it; break the square where the line crosses
x0, x1, y0, y1 = BX - MARK / 2, BX + MARK / 2, MARK_Y - MARK / 2, MARK_Y + MARK / 2; r = 2.0; k = r * (1 - math.sqrt(0.5)); lw = 0.38; gap = 0.9
seg((x0 + r, y0), (BX - gap, y0), B, lw); seg((BX + gap, y0), (x1 - r, y0), B, lw)
seg((x0 + r, y1), (BX - gap, y1), B, lw); seg((BX + gap, y1), (x1 - r, y1), B, lw)
seg((x1, y0 + r), (x1, y1 - r), B, lw); seg((x0, y1 - r), (x0, y0 + r), B, lw)
arc((x1 - r, y0), (x1 - k, y0 + k), (x1, y0 + r), B, lw); arc((x1, y1 - r), (x1 - k, y1 - k), (x1 - r, y1), B, lw)
arc((x0 + r, y1), (x0 + k, y1 - k), (x0, y1 - r), B, lw); arc((x0, y0 + r), (x0 + k, y0 + k), (x0 + r, y0), B, lw)
text('restorekit', 103.5, 124.0, B, 3.1, th=0.5, angle=90)
text('dongle pro', 108.4, 111.0, B, 1.15, th=0.2)
text('pd', 108.4, 112.6, B, 0.85)
text('liberate your mac', 119.6, 119.3, B, 1.0, angle=270, italic=True)
text('restorekit.org', BX, 139.1, B, 0.85)
text('HOST', CX, 102.3, B, 1.7, th=0.3)
text('the Mac that works', CX, 103.95, B, 0.8, w_ratio=0.85)
text('TARGET', CX, 146.0, B, 1.45, th=0.27)
text('the Mac that doesn\'t', CX, 144.0, B, 0.8, w_ratio=0.85)
text('LEFTSHIFT', 101.2, 143.4, B, 0.8, angle=90, w_ratio=0.85)
text('<< LOGICAL', 102.5, 143.4, B, 0.8, angle=90, w_ratio=0.85)
text('USB 3', 117.3, 110.0, B, 1.0, th=0.18)
text('flip-safe', 117.3, 111.5, B, 0.8, w_ratio=0.85)
text('CHARGER', 125.5, 102.3, B, 1.2, th=0.24)
text('up to 20 V', 125.5, 103.95, B, 0.8, w_ratio=0.85)

# ====================================================================== front
def pin1(ref, num, dx, dy):
    x, y = pad[(ref, num)]; dot(x + dx, y + dy, 0.18, F)
pin1('U1', '1', -0.55, 0.75); pin1('U2', '1', 0.55, 0.0); pin1('U3', '1', 0.0, -0.75); pin1('U4', '1', 0.0, 0.75)
pin1('U5', '1', -0.65, 0.0); pin1('U6', '1', 0.75, 0.4); pin1('U7', '1', -0.6, 0.9); pin1('U8', '1', -0.55, 0.0)
# ESD arrays: dot just past pin 1, in line with the pin row
for ref in ('D10', 'D11', 'D12', 'D13', 'D14', 'D15'):
    (x1, y1), (x2, y2) = pad[(ref, '1')], pad[(ref, '2')]
    l = math.hypot(x1 - x2, y1 - y2)
    dot(x1 + (x1 - x2) / l * 0.45, y1 + (y1 - y2) / l * 0.45, 0.15, F)
# LED cathode bars
for ref in ('D1', 'D2'):
    x, y = pad[(ref, '2')]; ax, ay = pad[(ref, '1')]
    ux, uy = (x - ax), (y - ay); l = math.hypot(ux, uy); ux, uy = ux / l, uy / l
    cx_, cy_ = x + ux * 0.7, y + uy * 0.7
    seg((cx_ - uy * 0.4, cy_ + ux * 0.4), (cx_ + uy * 0.4, cy_ - ux * 0.4), F, 0.15)
text('boot', 117.2, 135.7, F, 0.8)
text('act', 119.5, 129.6, F, 0.8, w_ratio=0.8)
text('pwr', 122.1, 129.4, F, 0.8, angle=270, w_ratio=0.8)
text('clk', 110.7, 124.55, F, 0.8, w_ratio=0.8); text('dio', 110.7, 126.6, F, 0.8, w_ratio=0.8)
text('3v3', 113.9, 121.8, F, 0.8, w_ratio=0.8); text('gnd', 116.7, 141.4, F, 0.8, w_ratio=0.8)
text('vbus', 120.3, 138.1, F, 0.8, w_ratio=0.8)
# top-left corner: the mark, small
mx, my, ms = 103.0, 103.9, 3.6
rrect(mx, my, ms, ms, 0.7, F, 0.2)
poly([(mx - 2.35, my), (mx - 1.2, my), (mx - 0.75, my - 1.0), (mx, my + 1.05), (mx + 0.6, my), (mx + 2.35, my)], F, 0.22)
text('restorekit', 102.85, 107.0, F, 0.8, th=0.14, w_ratio=0.78)
text('dongle pro', 102.85, 108.3, F, 0.8, w_ratio=0.78); text('pd', 102.85, 109.6, F, 0.8)
text('<host', 118.0, 102.2, F, 1.0, th=0.17)
text('pd in>', 118.0, 103.9, F, 0.9, th=0.15, w_ratio=0.85)
text('target', 118.0, 146.2, F, 1.1, th=0.18)
text('dfu port', 103.0, 146.2, F, 0.8)
text('charger', 127.4, 119.5, F, 1.1, th=0.18, angle=270)
text('20 V 3 A max', 125.9, 119.5, F, 0.8, w_ratio=0.8, angle=270)

pcbnew.SaveBoard(PCB, board)
import runpy
runpy.run_path(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py'))['write_pro']()
print('silk done')
sys.stdout.flush(); os._exit(0)
