#!/usr/bin/env python3
"""Reroute PROBE_SWCLK west section diagonally across the C9 GND wedge so the
pocket reconnects to the north B.Cu plane. Snapshot-before-remove. KiCad python."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

def near(a, bpt, tol=0.03):
    return math.hypot(a[0] - bpt[0], a[1] - bpt[1]) < tol

KILL_SEGS = [((105.4607, 115.9505), (105.0914, 115.5812)),
             ((105.0914, 115.5812), (100.5664, 115.5812)),
             ((100.5664, 115.5812), (100.5664, 129.8488)),
             ((100.5664, 129.8488), (101.3294, 130.6118))]
KILL_VIA = (100.5664, 115.5812)

items = []
for t in b.GetTracks():
    if t.GetNetname() != "PROBE_SWCLK":
        continue
    if t.GetClass() == "PCB_VIA":
        p = t.GetPosition()
        items.append((t, "via", (p.x/1e6, p.y/1e6), (p.x/1e6, p.y/1e6)))
    else:
        s, e = t.GetStart(), t.GetEnd()
        items.append((t, "trk", (s.x/1e6, s.y/1e6), (e.x/1e6, e.y/1e6)))

removed = 0
for obj, kind, s, e in items:
    kill = False
    if kind == "via" and near(s, KILL_VIA):
        kill = True
    elif kind == "trk":
        for a, c in KILL_SEGS:
            if (near(s, a) and near(e, c)) or (near(s, c) and near(e, a)):
                kill = True
    if kill:
        b.Remove(obj)
        removed += 1
print("removed:", removed)

def track(netname, layer, pts, w=0.15):
    net = _nets[netname]
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(P(x1, y1)); t.SetEnd(P(x2, y2))
        t.SetWidth(mm(w)); t.SetLayer(layer); t.SetNet(net)
        b.Add(t)

F, B = pcbnew.F_Cu, pcbnew.B_Cu
track("PROBE_SWCLK", B, [(105.4607, 115.9505), (103.5, 116.1), (102.35, 117.3),
                         (101.778, 117.75), (100.68, 118.55)])
v = pcbnew.PCB_VIA(b)
v.SetPosition(P(100.68, 118.55))
v.SetDrill(mm(0.3)); v.SetWidth(mm(0.5))
v.SetNet(_nets["PROBE_SWCLK"]); v.SetLayerPair(F, B)
b.Add(v)
track("PROBE_SWCLK", F, [(100.68, 118.55), (100.68, 129.83), (101.3294, 130.6118)])

pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
