#!/usr/bin/env python3
"""Dedupe, reconnect, clearance-fix, dangling cleanup, refill. KiCad python."""
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

_all = list(b.GetTracks())
removed = 0
seen_segs = set()
for t in _all:
    net = t.GetNetname()
    if t.GetClass() == "PCB_VIA":
        p = xy(t.GetPosition())
        if net == "VBUS" and p in ((108.0, 111.722), (105.323, 111.665)):
            b.Remove(t); removed += 1
        continue
    s, e = xy(t.GetStart()), xy(t.GetEnd())
    key = (net, t.GetLayerName(), tuple(sorted((s, e))))
    if net == "VBUS" and tuple(sorted((s, e))) in (
        tuple(sorted(((105.598, 111.391), (105.323, 111.665)))),
        tuple(sorted(((105.323, 111.082), (105.323, 111.665)))),
        tuple(sorted(((105.6, 109.9), (103.355, 109.9)))),
        tuple(sorted(((103.355, 109.9), (103.355, 110.55)))),
        tuple(sorted(((105.6, 109.005), (105.6, 109.9)))),
    ):
        b.Remove(t); removed += 1
        continue
    if net == "CC1":
        b.Remove(t); removed += 1
        continue
    if key in seen_segs:
        b.Remove(t); removed += 1
        continue
    seen_segs.add(key)
print("removed:", removed)

def track(netname, layer, pts, w=0.15):
    net = _nets[netname]
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(P(x1, y1)); t.SetEnd(P(x2, y2))
        t.SetWidth(mm(w)); t.SetLayer(layer); t.SetNet(net)
        b.Add(t)

def via(netname, x, y):
    v = pcbnew.PCB_VIA(b)
    v.SetPosition(P(x, y))
    v.SetDrill(mm(0.3)); v.SetWidth(mm(0.6)); v.SetNet(_nets[netname])
    v.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
    b.Add(v)

F, B = pcbnew.F_Cu, pcbnew.B_Cu

# VBUS feed, dodging the B1A12 blade above and R2 pads below
track("VBUS", F, [(105.6, 109.005), (105.6, 109.99), (103.3545, 109.99), (103.3545, 110.5501)], w=0.25)
# restore the LDO-side loop the rip took out (VIN pad + onward chain)
track("VBUS", F, [(102.534, 110.55), (102.534, 112.45), (103.3545, 112.4501)], w=0.16)
# DP bridge: extend the orphan B.Cu run to a via, tie into the pad1 column
track("USB_DP", B, [(107.296, 111.882), (107.35, 112.15)])
via("USB_DP", 107.35, 112.15)
track("USB_DP", F, [(107.35, 112.15), (107.05, 112.45)])
# CC1 rebuilt with a wider berth around D3 pad 4
track("CC1", F, [(109.25, 109.005), (109.25, 109.6), (109.55, 109.9), (109.55, 110.52), (110.89, 110.52)])

# dangling cleanup: endpoint must touch another track span, via, or pad
def cleanup():
    tracks = [t for t in b.GetTracks() if t.GetClass() == "PCB_TRACK"]
    vias = [(t.GetPosition().x / 1e6, t.GetPosition().y / 1e6) for t in b.GetTracks() if t.GetClass() == "PCB_VIA"]
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
            touched = False
            for o in tracks:
                if o is t or o.GetNetname() != t.GetNetname():
                    continue
                s, e = o.GetStart(), o.GetEnd()
                if seg_dist(px, py, s.x/1e6, s.y/1e6, e.x/1e6, e.y/1e6) < 0.08:
                    touched = True
                    break
            if not touched:
                touched = any(math.hypot(px - vx, py - vy) < 0.31 for vx, vy in vias)
            if not touched:
                touched = any(p.HitTest(pt) for p in pads)
            if touched:
                ok_ends += 1
        if ok_ends < 2:
            b.Remove(t)
            n += 1
    return n

total = 0
for _ in range(6):
    n = cleanup()
    total += n
    if n == 0:
        break
print("dangling removed:", total)

pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
