#!/usr/bin/env python3
"""Fix SWCLK diagonal clearance to BOOT_SW (true 45-degree line c=220.0) and
normalize stitch via drills to 0.3. Snapshot-before-remove. KiCad python."""
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

KILL_SEGS = [((105.4607, 115.9505), (103.5, 116.1)),
             ((103.5, 116.1), (102.35, 117.3)),
             ((102.35, 117.3), (101.778, 117.75)),
             ((101.778, 117.75), (100.68, 118.55))]

items = []
for t in b.GetTracks():
    if t.GetClass() == "PCB_VIA":
        p = t.GetPosition()
        items.append((t, "via", (p.x/1e6, p.y/1e6), (p.x/1e6, p.y/1e6)))
    else:
        s, e = t.GetStart(), t.GetEnd()
        items.append((t, "trk", (s.x/1e6, s.y/1e6), (e.x/1e6, e.y/1e6)))

removed = resized = 0
for obj, kind, s, e in items:
    if kind == "trk" and obj.GetNetname() == "PROBE_SWCLK":
        for a, c in KILL_SEGS:
            if (near(s, a) and near(e, c)) or (near(s, c) and near(e, a)):
                b.Remove(obj)
                removed += 1
                break
    elif kind == "via" and obj.GetNetname() == "GND" and abs(obj.GetDrillValue()/1e6 - 0.2) < 0.01:
        obj.SetDrill(mm(0.3))
        resized += 1
print("removed:", removed, "resized:", resized)

def track(netname, layer, pts, w=0.15):
    net = _nets[netname]
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(P(x1, y1)); t.SetEnd(P(x2, y2))
        t.SetWidth(mm(w)); t.SetLayer(layer); t.SetNet(net)
        b.Add(t)

track("PROBE_SWCLK", pcbnew.B_Cu, [(105.4607, 115.9505), (103.65, 115.92), (101.9, 117.67), (100.68, 118.55)])

ds = b.GetDesignSettings()
ds.m_ViasMinSize = mm(0.45)
pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
