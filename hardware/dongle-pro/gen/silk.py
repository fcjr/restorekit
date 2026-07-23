#!/usr/bin/env python3
"""Board-level silkscreen: functional labels on the front, artwork on the back.
Idempotent: wipes all board-level (non-footprint) silk items first.
Run with KiCad's bundled python after gen/zones.py.
"""
import os
os.chdir(os.path.join(os.path.dirname(__file__), '..'))
import pcbnew

PCB = 'dongle-pro.kicad_pcb'
board = pcbnew.LoadBoard(PCB)
F, B = pcbnew.F_SilkS, pcbnew.B_SilkS
MM = pcbnew.FromMM
V = pcbnew.VECTOR2I_MM

FPS = list(board.GetFootprints())
pads = {}
for fp in FPS:
    p = fp.GetPosition()
    pads[fp.GetReference()] = (p.x / 1e6, p.y / 1e6, fp)

for d in list(board.Drawings()):
    if d.GetLayer() in (F, B):
        board.Remove(d)

def text(s, x, y, layer, size, th=None, angle=0, w_ratio=0.85):
    t = pcbnew.PCB_TEXT(board)
    t.SetText(s); t.SetPosition(V(x, y)); t.SetLayer(layer)
    t.SetTextSize(pcbnew.VECTOR2I(MM(size * w_ratio), MM(size)))
    t.SetTextThickness(MM(th if th else size * 0.16))
    t.SetTextAngle(pcbnew.EDA_ANGLE(angle))
    if layer == B:
        t.SetMirrored(True)
    board.Add(t)
    return t

def seg(x1, y1, x2, y2, layer, w=0.25):
    s = pcbnew.PCB_SHAPE(board)
    s.SetShape(pcbnew.SHAPE_T_SEGMENT)
    s.SetStart(V(x1, y1)); s.SetEnd(V(x2, y2))
    s.SetLayer(layer); s.SetWidth(MM(w))
    board.Add(s)

def polyline(pts, layer, w=0.25):
    for a, b2 in zip(pts, pts[1:]):
        seg(a[0], a[1], b2[0], b2[1], layer, w)

def arc(start, mid, end, layer, w=0.25):
    s = pcbnew.PCB_SHAPE(board)
    s.SetShape(pcbnew.SHAPE_T_ARC)
    s.SetArcGeometry(V(*start), V(*mid), V(*end))
    s.SetLayer(layer); s.SetWidth(MM(w))
    board.Add(s)

def dot(x, y, r, layer):
    s = pcbnew.PCB_SHAPE(board)
    s.SetShape(pcbnew.SHAPE_T_CIRCLE)
    s.SetStart(V(x, y)); s.SetEnd(V(x + r, y))
    s.SetFilled(True); s.SetLayer(layer); s.SetWidth(0)
    board.Add(s)

def poly(pts, layer):
    s = pcbnew.PCB_SHAPE(board)
    s.SetShape(pcbnew.SHAPE_T_POLY)
    chain = pcbnew.SHAPE_LINE_CHAIN()
    for x, y in pts:
        chain.Append(V(x, y))
    chain.SetClosed(True)
    s.SetPolyShape(pcbnew.SHAPE_POLY_SET(chain))
    s.SetFilled(True); s.SetLayer(layer); s.SetWidth(0)
    board.Add(s)

def bolt(cx, cy, sc, layer):
    """Lightning bolt, roughly sc wide x 2*sc tall, centered."""
    pts = [(0.20, -1.0), (0.55, -1.0), (0.10, -0.15), (0.45, -0.15),
           (-0.20, 1.0), (-0.05, 0.25), (-0.45, 0.25)]
    poly([(cx + px * sc, cy + py * sc) for px, py in pts], layer)

CX = 109.0  # board centerline

# ============================ FRONT ============================
text('HOST', CX, 108.7, F, 1.5, 0.28)
text('TARGET', CX, 187.15, F, 1.2, 0.22)
text('DONGLE-PRO  v1.0', 97.6, 125.0, F, 1.8, 0.3, angle=-90)

if 'SW1' in pads:
    x, y, _ = pads['SW1']
    text('BOOT', x, y + 4.4, F, 1.0, 0.18)
if 'J3' in pads:
    x, y, _ = pads['J3']
    text('DBG', x + 1.9, y, F, 1.0, 0.18, angle=90)
if 'D1' in pads:
    x, y, _ = pads['D1']
    text('PWR', x, y - 1.3, F, 0.8, 0.15)
if 'D2' in pads:
    x, y, _ = pads['D2']
    text('ST', x - 0.8, y - 1.45, F, 0.8, 0.15)

TP_LABEL = {'TP1': '5V', 'TP2': '3V3', 'TP3': '1V2', 'TP4': '1V1',
            'TP5': 'VBUS', 'TP6': 'GND', 'TP7': 'RUN', 'TP8': 'INT',
            'TP9': 'H3V3', 'TP10': 'H1V2', 'TP11': 'RST'}
TP_OFF = {'TP9': (-2.6, 0), 'TP10': (-2.6, 0), 'TP11': (-2.6, 0),
          'TP4': (0, -1.15), 'TP5': (0, -1.95), 'TP6': (0, -1.15),
          'TP7': (2.2, -0.75), 'TP8': (0, -1.6)}
for ref, label in TP_LABEL.items():
    if ref not in pads:
        continue
    x, y, _ = pads[ref]
    dx, dy = TP_OFF.get(ref, (0, 1.5 if y < 148 else -1.5))
    text(label, x + dx, y + dy, F, 0.7, 0.13)

# ============================ BACK =============================
text('HOST', 119.8, 104.8, B, 2.2, 0.42, angle=90)

# --- eye diagram, y 108.5..117.5 ---
ey0, ey1 = 108.5, 117.5
em = (ey0 + ey1) / 2
exl, exr = 101.5, 116.5
xc1, xc2 = exl + 2.0, exr - 2.0
seg(exl, ey0 + 0.6, xc1, em, B, 0.3); seg(exl, ey1 - 0.6, xc1, em, B, 0.3)
seg(exr, ey0 + 0.6, xc2, em, B, 0.3); seg(exr, ey1 - 0.6, xc2, em, B, 0.3)
exm = (xc1 + xc2) / 2
arc((xc1, em), (exm, ey0 + 0.9), (xc2, em), B, 0.3)
arc((xc2, em), (exm, ey1 - 0.9), (xc1, em), B, 0.3)
text('5 Gb/s', 119.3, em, B, 1.6, 0.3, angle=-90)
text('USB 3.1 GEN 1', CX, 120.6, B, 1.7, 0.3)

# --- wordmark ---
text('DONGLE-PRO', CX, 148.5, B, 6.2, 1.1, angle=90, w_ratio=0.8)
bolt(100.6, 148.5, 3.2, B)
bolt(117.4, 148.5, 3.2, B)

text('RecoverKit', CX, 176.8, B, 2.2, 0.4)
text('restorekit.org', CX, 179.1, B, 1.6, 0.28)

# --- differential pair serpentine with a lane-flip crossover ---
g = 0.55     # half-gap of the pair
w = 0.35
yA, yB = 181.6, 187.2
polyline([(99.8, yA - g), (106.2, yA - g)], B, w)
polyline([(99.8, yA + g), (105.7, yA + g)], B, w)
# crossover X (the lane-mux flip)
seg(106.2, yA - g, 108.4, yA + g, B, w)
seg(105.7, yA + g, 107.9, yA - g, B, w)
# sweep right and down to row B with generous 45-deg turns
tx = 115.4
polyline([(108.4, yA + g), (tx, yA + g), (tx + 2.2 - g, yA + g + 2.2),
          (tx + 2.2 - g, yB - g - 2.2), (tx, yB - g), (99.8, yB - g)], B, w)
polyline([(107.9, yA - g), (tx + 1.1, yA - g), (tx + 3.3 - g, yA - g + 2.2 + 2 * g),
          (tx + 3.3 - g, yB + g - 2.2 - 2 * g), (tx + 1.1, yB + g), (99.8, yB + g)], B, w)
dot(99.8, yA - g, 0.5, B); dot(99.8, yA + g, 0.5, B)
dot(99.8, yB - g, 0.5, B); dot(99.8, yB + g, 0.5, B)
text('CC1', 97.9, yA, B, 0.9, 0.16, angle=-90)
text('CC2', 97.9, yB, B, 0.9, 0.16, angle=-90)

text('TARGET', 119.8, 190.3, B, 2.2, 0.42, angle=90)
text('v1.0', CX, 194.5, B, 1.3, 0.22)

pcbnew.SaveBoard(PCB, board)
_pro_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py')
pro = {'__file__': _pro_path}
exec(compile(open(_pro_path).read(), 'project.py', 'exec'), pro)
pro['write_pro']()
import subprocess, sys
subprocess.run([sys.executable,
                os.path.join(os.path.dirname(os.path.abspath(__file__)), 'tidy.py')],
               check=True)
print('silkscreen applied')
