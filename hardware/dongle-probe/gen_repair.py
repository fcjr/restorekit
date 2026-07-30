#!/usr/bin/env python3
"""Bridge disconnected clusters on damaged nets (additive only). KiCad python."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
NETS = ["VBUS", "XIN", "PROBE_BOOT", "PROBE_SWCLK", "PROBE_RESET", "QSPI_SS"]

b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()

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

# obstacle snapshot (other nets) for bridge collision checks
obstacles = []
for t in b.GetTracks():
    if t.GetClass() == "PCB_VIA":
        p = t.GetPosition()
        obstacles.append((t.GetNetname(), "BOTH", p.x/1e6, p.y/1e6, p.x/1e6, p.y/1e6, t.GetWidth(pcbnew.F_Cu)/2e6))
    else:
        s, e = t.GetStart(), t.GetEnd()
        obstacles.append((t.GetNetname(), t.GetLayerName(), s.x/1e6, s.y/1e6, e.x/1e6, e.y/1e6, t.GetWidth()/2e6))
pad_boxes = []
for fp in b.GetFootprints():
    for p in fp.Pads():
        bb = p.GetBoundingBox()
        pad_boxes.append((p.GetNetname(), bb.GetLeft()/1e6, bb.GetTop()/1e6, bb.GetRight()/1e6, bb.GetBottom()/1e6))

def bridge_clear(netname, layer, x1, y1, x2, y2):
    steps = max(2, int(math.hypot(x2-x1, y2-y1) / 0.1))
    for i in range(steps + 1):
        x = x1 + (x2-x1) * i / steps
        y = y1 + (y2-y1) * i / steps
        for net, lay, ax, ay, bx, by, hw in obstacles:
            if net == netname or (lay != "BOTH" and lay != layer):
                continue
            if seg_dist(x, y, ax, ay, bx, by) < 0.075 + 0.13 + hw:
                return False
        for net, px0, py0, px1, py1 in pad_boxes:
            if net == netname:
                continue
            if px0 - 0.205 < x < px1 + 0.205 and py0 - 0.205 < y < py1 + 0.205:
                return False
    return True

added = []
for netname in NETS:
    tracks = []
    for t in b.GetTracks():
        if t.GetNetname() != netname:
            continue
        if t.GetClass() == "PCB_VIA":
            p = t.GetPosition()
            tracks.append({"kind": "via", "layer": "BOTH", "s": (p.x/1e6, p.y/1e6), "e": (p.x/1e6, p.y/1e6)})
        else:
            s, e = t.GetStart(), t.GetEnd()
            tracks.append({"kind": "trk", "layer": t.GetLayerName(),
                           "s": (s.x/1e6, s.y/1e6), "e": (e.x/1e6, e.y/1e6)})
    net_pads = []
    for fp in b.GetFootprints():
        for p in fp.Pads():
            if p.GetNetname() == netname:
                q = p.GetPosition()
                net_pads.append({"kind": "pad", "layer": "F.Cu", "obj": p,
                                 "s": (q.x/1e6, q.y/1e6), "e": (q.x/1e6, q.y/1e6),
                                 "ref": "%s.%s" % (fp.GetReference(), p.GetName())})
    nodes = tracks + net_pads
    parent = list(range(len(nodes)))
    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i
    def union(i, j):
        parent[find(i)] = find(j)
    def touches(a, c):
        if a["kind"] == "pad" and c["kind"] == "pad":
            return False
        if a["kind"] == "pad":
            a, c = c, a
        if c["kind"] == "pad":
            for pt in (a["s"], a["e"]):
                if c["obj"].HitTest(P(pt[0], pt[1])):
                    return True
            return False
        if a["layer"] != "BOTH" and c["layer"] != "BOTH" and a["layer"] != c["layer"]:
            return False
        for pt in (a["s"], a["e"]):
            if seg_dist(pt[0], pt[1], c["s"][0], c["s"][1], c["e"][0], c["e"][1]) < 0.09:
                return True
        for pt in (c["s"], c["e"]):
            if seg_dist(pt[0], pt[1], a["s"][0], a["s"][1], a["e"][0], a["e"][1]) < 0.09:
                return True
        return False
    for i in range(len(nodes)):
        for j in range(i + 1, len(nodes)):
            if find(i) != find(j) and touches(nodes[i], nodes[j]):
                union(i, j)
    clusters = {}
    for i, n in enumerate(nodes):
        clusters.setdefault(find(i), []).append(n)
    groups = list(clusters.values())
    while len(groups) > 1:
        best = None
        for gi in range(1, len(groups)):
            for a in groups[0]:
                for c in groups[gi]:
                    for pa in (a["s"], a["e"]):
                        for pc in (c["s"], c["e"]):
                            d = math.hypot(pa[0]-pc[0], pa[1]-pc[1])
                            la = a["layer"]; lc = c["layer"]
                            layer = la if la != "BOTH" else lc
                            if lc != "BOTH" and la != "BOTH" and la != lc:
                                continue
                            if layer == "BOTH":
                                layer = "F.Cu"
                            if best is None or d < best[0]:
                                best = (d, pa, pc, layer, gi)
        if best is None or best[0] > 2.5:
            print(netname, "UNFIXED", None if best is None else best[0])
            break
        d, pa, pc, layer, gi = best
        lay = pcbnew.F_Cu if layer == "F.Cu" else pcbnew.B_Cu
        if bridge_clear(netname, layer, pa[0], pa[1], pc[0], pc[1]):
            t = pcbnew.PCB_TRACK(b)
            t.SetStart(P(pa[0], pa[1])); t.SetEnd(P(pc[0], pc[1]))
            t.SetWidth(mm(0.15)); t.SetLayer(lay); t.SetNet(_nets[netname])
            b.Add(t)
            added.append((netname, pa, pc, layer, round(d, 3)))
            groups[0].extend(groups.pop(gi))
            groups[0].append({"kind": "trk", "layer": layer, "s": pa, "e": pc})
        else:
            print(netname, "bridge blocked", pa, pc, layer)
            break

for a in added:
    print("bridged:", a)
pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
