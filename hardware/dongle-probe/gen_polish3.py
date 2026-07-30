#!/usr/bin/env python3
"""Final clearance fixes + cleanup, geometry snapshotted before any Remove. KiCad python."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

# geometry snapshot BEFORE any mutation
items = []   # dict: obj, cls, net, layer, s, e (for via: s == e == pos)
for t in b.GetTracks():
    if t.GetClass() == "PCB_VIA":
        p = t.GetPosition()
        items.append({"o": t, "cls": "VIA", "net": t.GetNetname(), "layer": "BOTH",
                      "s": (p.x / 1e6, p.y / 1e6), "e": (p.x / 1e6, p.y / 1e6)})
    else:
        s, e = t.GetStart(), t.GetEnd()
        items.append({"o": t, "cls": "TRK", "net": t.GetNetname(), "layer": t.GetLayerName(),
                      "s": (s.x / 1e6, s.y / 1e6), "e": (e.x / 1e6, e.y / 1e6)})
pads = []
pad_objs = [p for fp in b.GetFootprints() for p in fp.Pads()]

STUB_POS = [(107.827, 120.1918), (108.0746, 124.6609), (108.094, 119.9248), (107.5203, 124.6609),
            (109.7371, 133.845), (100.5664, 115.5812), (100.5664, 116.0927), (103.7408, 121.1869),
            (103.3824, 121.1869), (113.2401, 111.4901), (108.0, 111.722), (107.05, 111.319)]

def near(a, bpt, tol=0.05):
    return math.hypot(a[0] - bpt[0], a[1] - bpt[1]) < tol

def sortkey(s, e):
    return tuple(sorted((tuple(round(v, 3) for v in s), tuple(round(v, 3) for v in e))))

KILL_VBUS = {sortkey((105.6, 109.005), (105.6, 109.99)),
             sortkey((105.6, 109.99), (103.354, 109.99)),
             sortkey((105.6, 109.99), (103.355, 109.99)),
             sortkey((103.354, 109.99), (103.354, 110.55)),
             sortkey((103.355, 109.99), (103.355, 110.55))}
KILL_DP = {sortkey((107.296, 111.882), (107.35, 112.15)),
           sortkey((107.35, 112.15), (107.05, 112.45))}

removed = 0
alive = []
for it in items:
    kill = False
    if it["cls"] == "VIA":
        if it["net"] == "USB_DP" and near(it["s"], (107.35, 112.15)):
            kill = True
    else:
        ln = math.hypot(it["s"][0] - it["e"][0], it["s"][1] - it["e"][1])
        k = sortkey(it["s"], it["e"])
        if ln < 0.01:
            kill = True
        elif any(near(it["s"], sp) or near(it["e"], sp) for sp in STUB_POS):
            kill = True
        elif it["net"] == "VBUS" and k in KILL_VBUS:
            kill = True
        elif it["net"] == "USB_DP" and k in KILL_DP:
            kill = True
    if kill:
        b.Remove(it["o"])
        removed += 1
    else:
        alive.append(it)
print("removed:", removed)

def add_track(netname, layer, pts, w=0.15):
    net = _nets[netname]
    lname = "F.Cu" if layer == pcbnew.F_Cu else "B.Cu"
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(P(x1, y1)); t.SetEnd(P(x2, y2))
        t.SetWidth(mm(w)); t.SetLayer(layer); t.SetNet(net)
        b.Add(t)
        alive.append({"o": t, "cls": "TRK", "net": netname, "layer": lname, "s": (x1, y1), "e": (x2, y2)})

def add_via(netname, x, y):
    v = pcbnew.PCB_VIA(b)
    v.SetPosition(P(x, y))
    v.SetDrill(mm(0.3)); v.SetWidth(mm(0.6))
    v.SetNet(_nets[netname]); v.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
    b.Add(v)
    alive.append({"o": v, "cls": "VIA", "net": netname, "layer": "BOTH", "s": (x, y), "e": (x, y)})

add_track("VBUS", pcbnew.F_Cu, [(105.6, 109.005), (105.6, 109.95), (103.3545, 109.95), (103.3545, 110.5501)])
add_track("USB_DP", pcbnew.B_Cu, [(107.296, 111.882), (107.25, 112.1)])
add_via("USB_DP", 107.25, 112.1)
add_track("USB_DP", pcbnew.F_Cu, [(107.25, 112.1), (107.05, 112.3)])

def seg_dist(px, py, x1, y1, x2, y2):
    dx, dy = x2 - x1, y2 - y1
    if dx == dy == 0:
        return math.hypot(px - x1, py - y1)
    t = max(0, min(1, ((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (x1 + t * dx), py - (y1 + t * dy))

def cleanup_pass():
    n = 0
    tracks = [it for it in alive if it["cls"] == "TRK"]
    vias = [it for it in alive if it["cls"] == "VIA"]
    for it in list(tracks):
        ok = 0
        for pt in (it["s"], it["e"]):
            touched = any(o is not it and o["net"] == it["net"] and
                          seg_dist(pt[0], pt[1], o["s"][0], o["s"][1], o["e"][0], o["e"][1]) < 0.08
                          for o in tracks)
            if not touched:
                touched = any(near(pt, v["s"], 0.31) for v in vias if v["net"] == it["net"])
            if not touched:
                vec = P(pt[0], pt[1])
                touched = any(p.HitTest(vec) for p in pad_objs)
            if touched:
                ok += 1
        if ok < 2:
            b.Remove(it["o"])
            alive.remove(it)
            n += 1
    # orphan signal vias
    tracks = [it for it in alive if it["cls"] == "TRK"]
    for v in [it for it in alive if it["cls"] == "VIA"]:
        if v["net"] == "GND":
            continue
        touch = sum(1 for t in tracks if t["net"] == v["net"] and
                    (near(t["s"], v["s"], 0.31) or near(t["e"], v["s"], 0.31)))
        if touch < 2:
            b.Remove(v["o"])
            alive.remove(v)
            n += 1
    return n

total = 0
for _ in range(6):
    n = cleanup_pass()
    total += n
    if n == 0:
        break
print("cleaned:", total)

pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
