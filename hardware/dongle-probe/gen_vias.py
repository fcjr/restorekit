#!/usr/bin/env python3
"""Collision-checked GND via placement + zone refill. KiCad python."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()
gnd = _nets["GND"]

def mm(v):
    return pcbnew.FromMM(v)

# drop all existing GND vias (previous attempt); signal vias from the router stay
_all = list(b.GetTracks())
_gnd_vias = [t for t in _all if t.GetClass() == "PCB_VIA" and t.GetNetname() == "GND"]
for t in _gnd_vias:
    b.Remove(t)
_tracks = [t for t in _all if t not in _gnd_vias]

VIA_R = 0.3      # via copper radius
CLEAR = 0.18     # required copper gap
HOLE_GAP = 0.35  # hole-to-hole

segs = []        # (layer, x1,y1,x2,y2, halfwidth, net)
holes = []       # (x, y, r)
for t in _tracks:
    if t.GetClass() == "PCB_VIA":
        p = t.GetPosition()
        holes.append((p.x/1e6, p.y/1e6, t.GetDrillValue()/2e6))
        segs.append(("BOTH", p.x/1e6, p.y/1e6, p.x/1e6, p.y/1e6, t.GetWidth()/2e6, t.GetNetname()))
    else:
        s, e = t.GetStart(), t.GetEnd()
        segs.append((t.GetLayerName(), s.x/1e6, s.y/1e6, e.x/1e6, e.y/1e6, t.GetWidth()/2e6, t.GetNetname()))

pads = []        # (x0,y0,x1,y1, net, layerset_has_F, has_B, is_th)
for fp in b.GetFootprints():
    for p in fp.Pads():
        bb = p.GetBoundingBox()
        th = p.GetDrillSizeX() > 0
        if th:
            pp = p.GetPosition()
            holes.append((pp.x/1e6, pp.y/1e6, p.GetDrillSizeX()/2e6))
        pads.append((bb.GetLeft()/1e6, bb.GetTop()/1e6, bb.GetRight()/1e6, bb.GetBottom()/1e6,
                     p.GetNetname(), th))

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
        if net == "GND":
            continue
        if seg_dist(x, y, x1, y1, x2, y2) < VIA_R + CLEAR + hw:
            return False
    for hx, hy, hr in holes:
        if math.hypot(x - hx, y - hy) < HOLE_GAP + 0.15 + hr:
            return False
    for x0, y0, x1, y1, net, th in pads:
        if net == "GND" and not th:
            continue
        cx = max(x0 - VIA_R - CLEAR, min(x, x1 + VIA_R + CLEAR))
        cy = max(y0 - VIA_R - CLEAR, min(y, y1 + VIA_R + CLEAR))
        if x0 - VIA_R - CLEAR < x < x1 + VIA_R + CLEAR and y0 - VIA_R - CLEAR < y < y1 + VIA_R + CLEAR:
            return False
    return True

def corridor_ok(x1, y1, x2, y2):
    steps = max(2, int(math.hypot(x2 - x1, y2 - y1) / 0.2))
    for i in range(steps + 1):
        x = x1 + (x2 - x1) * i / steps
        y = y1 + (y2 - y1) * i / steps
        for layer, sx1, sy1, sx2, sy2, hw, net in segs:
            if net == "GND" or layer == "B.Cu":
                continue
            if seg_dist(x, y, sx1, sy1, sx2, sy2) < 0.075 + 0.15 + hw:
                return False
        for x0, y0, xx1, yy1, net, th in pads:
            if net == "GND":
                continue
            if x0 - 0.23 < x < xx1 + 0.23 and y0 - 0.23 < y < yy1 + 0.23:
                return False
    return True

def add_via(x, y):
    v = pcbnew.PCB_VIA(b)
    v.SetPosition(pcbnew.VECTOR2I(mm(x), mm(y)))
    v.SetDrill(mm(0.3))
    v.SetWidth(mm(0.6))
    v.SetNet(gnd)
    v.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
    b.Add(v)
    holes.append((x, y, 0.15))
    segs.append(("BOTH", x, y, x, y, 0.3, "GND"))

def add_track(x1, y1, x2, y2):
    t = pcbnew.PCB_TRACK(b)
    t.SetStart(pcbnew.VECTOR2I(mm(x1), mm(y1)))
    t.SetEnd(pcbnew.VECTOR2I(mm(x2), mm(y2)))
    t.SetWidth(mm(0.15))
    t.SetLayer(pcbnew.F_Cu)
    t.SetNet(gnd)
    b.Add(t)
    segs.append(("F.Cu", x1, y1, x2, y2, 0.075, "GND"))

placed, failed = 0, []
gnd_smd = []
for fp in b.GetFootprints():
    for p in fp.Pads():
        if p.GetNetname() == "GND" and p.GetDrillSizeX() == 0 and p.IsOnLayer(pcbnew.F_Cu):
            gnd_smd.append((fp.GetReference(), p))

# EP first: vias inside the exposed pad where the bottom layer is clear
u1 = b.FindFootprintByReference("U1")
ep = [p for p in u1.Pads() if p.GetName() == "61"][0]
epp = ep.GetPosition()
ex, ey = epp.x/1e6, epp.y/1e6
ep_done = 0
for dx, dy in [(-0.85, -0.85), (0.85, -0.85), (-0.85, 0.85), (0.85, 0.85), (0, 0)]:
    x, y = ex + dx, ey + dy
    ok = True
    for layer, x1, y1, x2, y2, hw, net in segs:
        if net == "GND" or layer == "F.Cu":
            continue
        if seg_dist(x, y, x1, y1, x2, y2) < VIA_R + CLEAR + hw:
            ok = False
            break
    if ok:
        for hx, hy, hr in holes:
            if math.hypot(x - hx, y - hy) < HOLE_GAP + 0.15 + hr:
                ok = False
                break
    if ok and ep_done < 4:
        add_via(x, y)
        ep_done += 1
print("EP vias:", ep_done)

seen = []
for ref, p in gnd_smd:
    pp = p.GetPosition()
    px, py = pp.x/1e6, pp.y/1e6
    if any(math.hypot(px - sx, py - sy) < 1.6 for sx, sy in seen):
        continue
    done = False
    for r in (0.7, 0.9, 1.1, 1.4, 1.7):
        for k in range(24):
            a = k * math.pi / 12
            x, y = px + r * math.cos(a), py + r * math.sin(a)
            if spot_ok(x, y) and corridor_ok(px, py, x, y):
                add_via(x, y)
                add_track(px, py, x, y)
                seen.append((x, y))
                placed += 1
                done = True
                break
        if done:
            break
    if not done:
        failed.append("%s.%s" % (ref, p.GetName()))
print("pad vias:", placed, "failed:", failed)

filler = pcbnew.ZONE_FILLER(b)
filler.Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
