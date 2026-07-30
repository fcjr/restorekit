#!/usr/bin/env python3
"""Report courtyard overlaps and board-edge margins. Run with KiCad's python."""
import pcbnew

b = pcbnew.LoadBoard("layout/layout.kicad_pcb")
boxes = {}
for fp in b.GetFootprints():
    bb = fp.GetCourtyard(pcbnew.F_CrtYd).BBox()
    if bb.GetWidth() == 0:
        bb = fp.GetBoundingBox(False)
    x0, y0, x1, y1 = bb.GetLeft(), bb.GetTop(), bb.GetRight(), bb.GetBottom()
    for pad in fp.Pads():
        pb = pad.GetBoundingBox()
        x0 = min(x0, pb.GetLeft()); y0 = min(y0, pb.GetTop())
        x1 = max(x1, pb.GetRight()); y1 = max(y1, pb.GetBottom())
    boxes[fp.GetReference()] = (x0/1e6, y0/1e6, x1/1e6, y1/1e6)

refs = sorted(boxes)
bad = 0
for i, a in enumerate(refs):
    ax0, ay0, ax1, ay1 = boxes[a]
    for c in refs[i+1:]:
        cx0, cy0, cx1, cy1 = boxes[c]
        ox = min(ax1, cx1) - max(ax0, cx0)
        oy = min(ay1, cy1) - max(ay0, cy0)
        if ox > 0.01 and oy > 0.01:
            print("OVERLAP %s %s by %.2f x %.2f mm" % (a, c, ox, oy))
            bad += 1
X0, Y0, X1, Y1 = 100.0, 100.0, 116.0, 143.0
for r in refs:
    x0, y0, x1, y1 = boxes[r]
    m = min(x0 - X0, y0 - Y0, X1 - x1, Y1 - y1)
    if m < 0.6 and r not in ("J1", "J2"):  # both connectors sit flush at an edge
        print("EDGE %s margin %.2f mm" % (r, m))
print("overlaps:", bad)
