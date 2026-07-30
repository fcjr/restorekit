#!/usr/bin/env python3
"""Drop a GND via into any filled F.Cu island lacking one, refill. KiCad python."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()
gnd = _nets["GND"]

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))


_tracks2 = list(b.GetTracks())
holes = []
segs = []
for t in _tracks2:
    if t.GetClass() == "PCB_VIA":
        p = t.GetPosition()
        holes.append((p.x/1e6, p.y/1e6, t.GetDrillValue()/2e6, t.GetNetname()))
        segs.append(("BOTH", p.x/1e6, p.y/1e6, p.x/1e6, p.y/1e6, t.GetWidth(pcbnew.F_Cu)/2e6, t.GetNetname()))
    else:
        s, e = t.GetStart(), t.GetEnd()
        segs.append((t.GetLayerName(), s.x/1e6, s.y/1e6, e.x/1e6, e.y/1e6, t.GetWidth()/2e6, t.GetNetname()))
pad_boxes = []
for fp in b.GetFootprints():
    for p in fp.Pads():
        bb = p.GetBoundingBox()
        if p.GetDrillSizeX() > 0:
            q = p.GetPosition()
            holes.append((q.x/1e6, q.y/1e6, p.GetDrillSizeX()/2e6, p.GetNetname()))
        pad_boxes.append((bb.GetLeft()/1e6, bb.GetTop()/1e6, bb.GetRight()/1e6, bb.GetBottom()/1e6, p.GetNetname()))

def seg_dist(px, py, x1, y1, x2, y2):
    dx, dy = x2 - x1, y2 - y1
    if dx == dy == 0:
        return math.hypot(px - x1, py - y1)
    t = max(0, min(1, ((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (x1 + t * dx), py - (y1 + t * dy))

def spot_ok(x, y):
    if not (100.5 <= x <= 115.5 and 100.5 <= y <= 137.9):
        return False
    for layer, x1, y1, x2, y2, hw, net in segs:
        if net != "GND" and seg_dist(x, y, x1, y1, x2, y2) < 0.3 + 0.18 + hw:
            return False
    for hx, hy, hr, net in holes:
        if math.hypot(x - hx, y - hy) < 0.35 + 0.15 + hr:
            return False
    for x0, y0, x1, y1, net in pad_boxes:
        if net == "GND":
            continue
        if x0 - 0.48 < x < x1 + 0.48 and y0 - 0.48 < y < y1 + 0.48:
            return False
    return True

added = 0
zones_f = [z for z in b.Zones() if pcbnew.F_Cu in list(z.GetLayerSet().Seq()) and pcbnew.B_Cu not in list(z.GetLayerSet().Seq())]
for z in zones_f:
    polys = z.GetFilledPolysList(pcbnew.F_Cu)
    for i in range(polys.OutlineCount()):
        ol = polys.COutline(i)
        bb = ol.BBox()
        has_via = any(bb.GetLeft()/1e6 <= hx <= bb.GetRight()/1e6 and
                      bb.GetTop()/1e6 <= hy <= bb.GetBottom()/1e6 and
                      net == "GND" and polys.Contains(pcbnew.VECTOR2I(int(hx*1e6), int(hy*1e6)), i)
                      for hx, hy, hr, net in holes)
        if has_via:
            continue
        gx0, gy0 = bb.GetLeft()/1e6, bb.GetTop()/1e6
        gx1, gy1 = bb.GetRight()/1e6, bb.GetBottom()/1e6
        done = False
        steps = 14
        for iy in range(steps + 1):
            for ix in range(steps + 1):
                x = gx0 + (gx1 - gx0) * ix / steps
                y = gy0 + (gy1 - gy0) * iy / steps
                if not polys.Contains(pcbnew.VECTOR2I(int(x*1e6), int(y*1e6)), i):
                    continue
                if spot_ok(x, y):
                    v = pcbnew.PCB_VIA(b)
                    v.SetPosition(P(x, y))
                    v.SetDrill(mm(0.3)); v.SetWidth(mm(0.6)); v.SetNet(gnd)
                    v.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
                    b.Add(v)
                    holes.append((x, y, 0.15, "GND"))
                    segs.append(("BOTH", x, y, x, y, 0.3, "GND"))
                    added += 1
                    done = True
                    break
            if done:
                break
print("island vias:", added)

filler = pcbnew.ZONE_FILLER(b)
filler.Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
