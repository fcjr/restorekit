#!/usr/bin/env python3
"""Silkscreen auto-tidy (run by gen/silk.py in a fresh process):
1. re-place every front reference field so nothing overlaps pads, silk
   graphics, board texts, or other refs;
2. strip footprint F-silk graphics that sit on copper pads or off-board.
Reads first, ref moves second, removals last — pcbnew's SWIG wrappers go
stale after removals, so the order matters.
"""
import os
os.chdir(os.path.join(os.path.dirname(__file__), '..'))
import pcbnew

PCB = 'dongle-pro.kicad_pcb'
board = pcbnew.LoadBoard(PCB)
F = pcbnew.F_SilkS
MM = pcbnew.FromMM
V = pcbnew.VECTOR2I_MM
WIN = (96.35, 99.35, 121.65, 196.65)

def bb2t(bb, grow=0.0):
    return (bb.GetLeft() / 1e6 - grow, bb.GetTop() / 1e6 - grow,
            bb.GetRight() / 1e6 + grow, bb.GetBottom() / 1e6 + grow)

def hits(a, b):
    return not (a[2] <= b[0] or b[2] <= a[0] or a[3] <= b[1] or b[3] <= a[1])

FPS = list(board.GetFootprints())

# ---------- reads ----------
pad_boxes = []
for fp in FPS:
    for pad in fp.Pads():
        if pad.IsOnLayer(pcbnew.F_Cu) or pad.GetAttribute() in (
                pcbnew.PAD_ATTRIB_PTH, pcbnew.PAD_ATTRIB_NPTH):
            pad_boxes.append(bb2t(pad.GetBoundingBox(), 0.1))

strip = []          # (fp, item) pairs removed at the very end
silk_boxes = []
for fp in FPS:
    for it in fp.GraphicalItems():
        if it.GetLayer() != F:
            continue
        t = bb2t(it.GetBoundingBox())
        off = (t[0] < WIN[0] or t[1] < WIN[1] or t[2] > WIN[2] or t[3] > WIN[3])
        onpad = any(hits(t, pb) for pb in pad_boxes)
        if off or onpad:
            strip.append((fp, it))
        else:
            silk_boxes.append(bb2t(it.GetBoundingBox(), 0.06))
for d in board.Drawings():
    if d.GetLayer() == F:
        silk_boxes.append(bb2t(d.GetBoundingBox(), 0.06))

# ---------- ref placement ----------
obstacles = pad_boxes + silk_boxes
moved = hidden = 0
for fp in sorted(FPS, key=lambda f: f.GetReference()):
    ref = fp.Reference()
    if ref.GetLayer() != F or not ref.IsVisible():
        continue
    ref.SetTextSize(pcbnew.VECTOR2I(MM(0.7), MM(0.7)))
    ref.SetTextThickness(MM(0.12))
    ref.SetTextAngle(pcbnew.EDA_ANGLE(0))
    fb = fp.GetBoundingBox(False, False)
    cx, cy = fb.GetCenter().x / 1e6, fb.GetCenter().y / 1e6
    hw, hh = fb.GetWidth() / 2e6, fb.GetHeight() / 2e6
    tb = ref.GetBoundingBox()
    tw, th = tb.GetWidth() / 2e6, tb.GetHeight() / 2e6
    cands = [None]
    for dist in (0.3, 0.6, 1.0, 1.5, 2.1, 2.8):
        cands += [(cx, cy - hh - dist - th), (cx, cy + hh + dist + th),
                  (cx - hw - dist - tw, cy), (cx + hw + dist + tw, cy),
                  (cx - hw - dist - tw, cy - hh - dist - th),
                  (cx + hw + dist + tw, cy - hh - dist - th),
                  (cx - hw - dist - tw, cy + hh + dist + th),
                  (cx + hw + dist + tw, cy + hh + dist + th)]
    placed = False
    for cand in cands:
        if cand is not None:
            ref.SetPosition(V(cand[0], cand[1]))
        t = bb2t(ref.GetBoundingBox(), 0.04)
        if t[0] < WIN[0] or t[1] < WIN[1] or t[2] > WIN[2] or t[3] > WIN[3]:
            continue
        if any(hits(t, ob) for ob in obstacles):
            continue
        placed = True
        if cand is not None:
            moved += 1
        break
    if not placed:
        ref.SetVisible(False)
        hidden += 1
        continue
    obstacles.append(bb2t(ref.GetBoundingBox(), 0.04))

# ---------- removals last ----------
for fp, it in strip:
    fp.Remove(it)

pcbnew.SaveBoard(PCB, board)
_pro_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py')
pro = {'__file__': _pro_path}
exec(compile(open(_pro_path).read(), 'project.py', 'exec'), pro)
pro['write_pro']()
print(f'tidy: moved {moved} refs, hid {hidden}, stripped {len(strip)} silk items')
