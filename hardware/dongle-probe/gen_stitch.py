#!/usr/bin/env python3
"""Union GND fill fragments + wired GND items; stitch disconnected components with vias. KiCad python."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
gnd = b.GetNetsByName()["GND"]

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

def seg_dist(px, py, x1, y1, x2, y2):
    dx, dy = x2 - x1, y2 - y1
    if dx == dy == 0:
        return math.hypot(px - x1, py - y1)
    t = max(0, min(1, ((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (x1 + t * dx), py - (y1 + t * dy))

# obstacles for candidate via spots
segs = []
holes = []
for t in b.GetTracks():
    if t.GetClass() == "PCB_VIA":
        p = t.GetPosition()
        holes.append((p.x/1e6, p.y/1e6, t.GetDrillValue()/2e6, t.GetNetname(), t.GetWidth(pcbnew.F_Cu)/2e6))
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
            holes.append((q.x/1e6, q.y/1e6, p.GetDrillSizeX()/2e6, p.GetNetname(), max(p.GetSizeX(), p.GetSizeY())/2e6))
        pad_boxes.append((bb.GetLeft()/1e6, bb.GetTop()/1e6, bb.GetRight()/1e6, bb.GetBottom()/1e6, p.GetNetname()))

def spot_ok(x, y):
    if not (100.5 <= x <= 115.5 and 100.5 <= y <= 137.9):
        return False
    for layer, x1, y1, x2, y2, hw, net in segs:
        if net != "GND" and seg_dist(x, y, x1, y1, x2, y2) < 0.3 + 0.135 + hw:
            return False
    for hx, hy, hr, net, cr in holes:
        d = math.hypot(x - hx, y - hy)
        if net == "GND":
            if d < 0.15 + 0.26 + hr:
                return False
        elif d < 0.3 + 0.135 + cr:
            return False
    for x0, y0, x1, y1, net in pad_boxes:
        if net == "GND":
            continue
        if x0 - 0.44 < x < x1 + 0.44 and y0 - 0.44 < y < y1 + 0.44:
            return False
    return True

# GND fill fragments per layer
frags = []  # (layerName, polys, outline_idx)
for z in b.Zones():
    if z.GetNetname() != "GND":
        continue
    for lay in list(z.GetLayerSet().Seq()):
        if lay not in (pcbnew.F_Cu, pcbnew.B_Cu):
            continue
        polys = z.GetFilledPolysList(lay)
        lname = "F.Cu" if lay == pcbnew.F_Cu else "B.Cu"
        for i in range(polys.OutlineCount()):
            frags.append([lname, polys, i])

def inside(fr, x, y):
    return fr[1].Contains(pcbnew.VECTOR2I(int(x*1e6), int(y*1e6)), fr[2])

# wired GND connectors
conns = []  # (kind, layers, pts)
for t in b.GetTracks():
    if t.GetNetname() != "GND":
        continue
    if t.GetClass() == "PCB_VIA":
        p = t.GetPosition()
        conns.append(("via", ("F.Cu", "B.Cu"), [(p.x/1e6, p.y/1e6)]))
    else:
        s, e = t.GetStart(), t.GetEnd()
        conns.append(("trk", (t.GetLayerName(),), [(s.x/1e6, s.y/1e6), (e.x/1e6, e.y/1e6)]))
for fp in b.GetFootprints():
    for p in fp.Pads():
        if p.GetNetname() != "GND":
            continue
        q = p.GetPosition()
        lays = ("F.Cu", "B.Cu") if p.GetDrillSizeX() > 0 else ("F.Cu",)
        conns.append(("pad", lays, [(q.x/1e6, q.y/1e6)]))

n = len(frags) + len(conns)
parent = list(range(n))
def find(i):
    while parent[i] != i:
        parent[i] = parent[parent[i]]
        i = parent[i]
    return i
def union(i, j):
    parent[find(i)] = find(j)

for ci, c in enumerate(conns):
    for fi, fr in enumerate(frags):
        if fr[0] not in c[1]:
            continue
        if any(inside(fr, x, y) for x, y in c[2]):
            union(len(frags) + ci, fi)
# connector-connector adjacency (tracks chains, via-track)
for i in range(len(conns)):
    for j in range(i + 1, len(conns)):
        a, c = conns[i], conns[j]
        if not set(a[1]) & set(c[1]):
            continue
        hit = False
        for pa in a[2]:
            for pc in c[2]:
                if math.hypot(pa[0]-pc[0], pa[1]-pc[1]) < 0.3:
                    hit = True
        if hit:
            union(len(frags) + i, len(frags) + j)

added = []
for _round in range(20):
    comps = {}
    for i in range(n):
        comps.setdefault(find(i), []).append(i)
    sizes = sorted(((len(v), k) for k, v in comps.items()), reverse=True)
    print("components:", [s for s, _ in sizes])
    if len(sizes) == 1:
        break
    progress = False
    for _, root in sizes[1:]:
        fixed = False
        for i in comps[root]:
            if i >= len(frags):
                continue
            fr = frags[i]
            other = "B.Cu" if fr[0] == "F.Cu" else "F.Cu"
            bb = fr[1].COutline(fr[2]).BBox()
            ix0, iy0 = bb.GetLeft()/1e6, bb.GetTop()/1e6
            ix1, iy1 = bb.GetRight()/1e6, bb.GetBottom()/1e6
            for fj, fr2 in enumerate(frags):
                if fr2[0] != other or find(fj) == find(i):
                    continue
                bb2 = fr2[1].COutline(fr2[2]).BBox()
                gx0 = max(ix0, bb2.GetLeft()/1e6); gy0 = max(iy0, bb2.GetTop()/1e6)
                gx1 = min(ix1, bb2.GetRight()/1e6); gy1 = min(iy1, bb2.GetBottom()/1e6)
                if gx0 >= gx1 or gy0 >= gy1:
                    continue
                sx = max(2, min(80, int((gx1 - gx0) / 0.1)))
                sy = max(2, min(80, int((gy1 - gy0) / 0.1)))
                for iy in range(sy + 1):
                    for ix in range(sx + 1):
                        x = gx0 + (gx1 - gx0) * ix / sx
                        y = gy0 + (gy1 - gy0) * iy / sy
                        if not inside(fr, x, y) or not inside(fr2, x, y) or not spot_ok(x, y):
                            continue
                        v = pcbnew.PCB_VIA(b)
                        v.SetPosition(P(x, y))
                        v.SetDrill(mm(0.3)); v.SetWidth(mm(0.6)); v.SetNet(gnd)
                        v.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
                        b.Add(v)
                        holes.append((x, y, 0.15, "GND", 0.3))
                        union(i, fj)
                        added.append((round(x,3), round(y,3), fr[0]))
                        fixed = True
                        break
                    if fixed:
                        break
                if fixed:
                    break
            if fixed:
                break
        progress = progress or fixed
    if not progress:
        print("NO PROGRESS; remaining:", len(sizes), "components")
        break

print("stitch vias:", added)
pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
