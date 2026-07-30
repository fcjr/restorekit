#!/usr/bin/env python3
"""Dump cluster structure of broken nets. KiCad python. Read-only."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
NETS = ["VBUS", "XIN", "PROBE_BOOT", "PROBE_SWCLK", "PROBE_RESET", "QSPI_SS"]
b = pcbnew.LoadBoard(BOARD)

def P(x, y):
    return pcbnew.VECTOR2I(pcbnew.FromMM(x), pcbnew.FromMM(y))

def seg_dist(px, py, x1, y1, x2, y2):
    dx, dy = x2 - x1, y2 - y1
    if dx == dy == 0:
        return math.hypot(px - x1, py - y1)
    t = max(0, min(1, ((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (x1 + t * dx), py - (y1 + t * dy))

for netname in NETS:
    nodes = []
    for t in b.GetTracks():
        if t.GetNetname() != netname:
            continue
        if t.GetClass() == "PCB_VIA":
            p = t.GetPosition()
            nodes.append({"kind": "via", "layer": "BOTH", "s": (p.x/1e6, p.y/1e6), "e": (p.x/1e6, p.y/1e6)})
        else:
            s, e = t.GetStart(), t.GetEnd()
            nodes.append({"kind": "trk", "layer": t.GetLayerName(),
                          "s": (s.x/1e6, s.y/1e6), "e": (e.x/1e6, e.y/1e6)})
    for fp in b.GetFootprints():
        for p in fp.Pads():
            if p.GetNetname() == netname:
                q = p.GetPosition()
                nodes.append({"kind": "pad", "layer": "F.Cu", "obj": p,
                              "s": (q.x/1e6, q.y/1e6), "e": (q.x/1e6, q.y/1e6),
                              "ref": "%s.%s" % (fp.GetReference(), p.GetName())})
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
    print("=== %s: %d clusters" % (netname, len(clusters)))
    for ci, grp in enumerate(clusters.values()):
        desc = []
        for n in grp:
            if n["kind"] == "pad":
                desc.append("pad %s@(%.3f,%.3f)" % (n["ref"], n["s"][0], n["s"][1]))
            elif n["kind"] == "via":
                desc.append("via@(%.3f,%.3f)" % (n["s"][0], n["s"][1]))
            else:
                desc.append("%s (%.3f,%.3f)-(%.3f,%.3f)" % (n["layer"], n["s"][0], n["s"][1], n["e"][0], n["e"][1]))
        print("  C%d [%d]:" % (ci, len(grp)), "; ".join(desc))
