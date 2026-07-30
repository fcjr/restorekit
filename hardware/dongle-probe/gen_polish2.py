#!/usr/bin/env python3
"""Final clearance fixes + stub/orphan-via cleanup + refill. KiCad python."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

def xy(v):
    return (round(v.x / 1e6, 3), round(v.y / 1e6, 3))

# stubs reported dangling by DRC (position of one end)
STUB_POS = [(107.827, 120.1918), (108.0746, 124.6609), (108.094, 119.9248), (107.5203, 124.6609),
            (109.7371, 133.845), (100.5664, 115.5812), (100.5664, 116.0927), (103.7408, 121.1869),
            (103.3824, 121.1869), (113.2401, 111.4901), (108.0, 111.722), (107.05, 111.319)]

_alive = list(b.GetTracks())
_added = []
def _rm(item):
    global removed
    b.Remove(item)
    _alive.remove(item)
    removed += 1

removed = 0
for t in list(_alive):
    if t.GetClass() == "PCB_VIA":
        p = xy(t.GetPosition())
        if t.GetNetname() == "USB_DP" and p == (107.35, 112.15):
            _rm(t)
        continue
    s, e = xy(t.GetStart()), xy(t.GetEnd())
    ln = math.hypot(s[0] - e[0], s[1] - e[1])
    if ln < 0.01:
        _rm(t)
        continue
    if any(math.hypot(s[0]-px, s[1]-py) < 0.05 or math.hypot(e[0]-px, e[1]-py) < 0.05 for px, py in STUB_POS):
        _rm(t)
        continue
    if t.GetNetname() == "VBUS" and tuple(sorted((s, e))) in (
        tuple(sorted(((105.6, 109.005), (105.6, 109.99)))),
        tuple(sorted(((105.6, 109.99), (103.354, 109.99)))),
        tuple(sorted(((105.6, 109.99), (103.355, 109.99)))),
        tuple(sorted(((103.354, 109.99), (103.354, 110.55)))),
        tuple(sorted(((103.355, 109.99), (103.355, 110.55)))),
    ):
        _rm(t)
        continue
    if t.GetNetname() == "USB_DP" and tuple(sorted((s, e))) in (
        tuple(sorted(((107.296, 111.882), (107.35, 112.15)))),
        tuple(sorted(((107.35, 112.15), (107.05, 112.45)))),
    ):
        _rm(t)
print("removed:", removed)

def track(netname, layer, pts, w=0.15):
    net = _nets[netname]
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(P(x1, y1)); t.SetEnd(P(x2, y2))
        t.SetWidth(mm(w)); t.SetLayer(layer); t.SetNet(net)
        b.Add(t)
        _alive.append(t)

F, B = pcbnew.F_Cu, pcbnew.B_Cu
# VBUS feed: blade drop, lane between blade bottoms and R2 pad tops, down to LDO EN pad
track("VBUS", F, [(105.6, 109.005), (105.6, 109.95), (103.3545, 109.95), (103.3545, 110.5501)])
# DP bridge with pad-2 clearance
track("USB_DP", B, [(107.296, 111.882), (107.25, 112.1)])
v = pcbnew.PCB_VIA(b)
v.SetPosition(P(107.25, 112.1)); v.SetDrill(mm(0.3)); v.SetWidth(mm(0.6))
v.SetNet(_nets["USB_DP"]); v.SetLayerPair(F, B)
b.Add(v)
_alive.append(v)
track("USB_DP", F, [(107.25, 112.1), (107.05, 112.3)])

# orphan signal vias: no track touching on either layer
def via_pass():
    tracks = [t for t in _alive if t.GetClass() == "PCB_TRACK"]
    vias = [t for t in _alive if t.GetClass() == "PCB_VIA"]
    n = 0
    for v in vias:
        if v.GetNetname() == "GND":
            continue
        p = v.GetPosition()
        touch = 0
        for t in tracks:
            if t.GetNetname() != v.GetNetname():
                continue
            s, e = t.GetStart(), t.GetEnd()
            for pt in (s, e):
                if math.hypot((pt.x - p.x)/1e6, (pt.y - p.y)/1e6) < 0.31:
                    touch += 1
                    break
        if touch < 2:
            b.Remove(v)
            _alive.remove(v)
            n += 1
    return n

def cleanup():
    tracks = [t for t in _alive if t.GetClass() == "PCB_TRACK"]
    vias = [(t.GetPosition().x / 1e6, t.GetPosition().y / 1e6) for t in _alive if t.GetClass() == "PCB_VIA"]
    pads = [p for fp in b.GetFootprints() for p in fp.Pads()]
    def seg_dist(px, py, x1, y1, x2, y2):
        dx, dy = x2 - x1, y2 - y1
        if dx == dy == 0:
            return math.hypot(px - x1, py - y1)
        t = max(0, min(1, ((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy)))
        return math.hypot(px - (x1 + t * dx), py - (y1 + t * dy))
    n = 0
    for t in tracks:
        ok_ends = 0
        for pt in (t.GetStart(), t.GetEnd()):
            px, py = pt.x / 1e6, pt.y / 1e6
            touched = any(
                o is not t and o.GetNetname() == t.GetNetname() and
                seg_dist(px, py, o.GetStart().x/1e6, o.GetStart().y/1e6, o.GetEnd().x/1e6, o.GetEnd().y/1e6) < 0.08
                for o in tracks)
            if not touched:
                touched = any(math.hypot(px - vx, py - vy) < 0.31 for vx, vy in vias)
            if not touched:
                touched = any(p.HitTest(pt) for p in pads)
            if touched:
                ok_ends += 1
        if ok_ends < 2:
            b.Remove(t)
            _alive.remove(t)
            n += 1
    return n

total = 0
for _ in range(6):
    n = cleanup() + via_pass()
    total += n
    if n == 0:
        break
print("cleaned:", total)

pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
