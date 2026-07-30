#!/usr/bin/env python3
"""Surgical routing fixes + zone solid connection + refill. KiCad python."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()
gnd = _nets["GND"]
dm = _nets["USB_DM"]

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

_all = list(b.GetTracks())
removed = 0

# 1) my bad F.Cu DM jumper through the occupied corridor
for t in _all:
    if t.GetClass() == "PCB_TRACK" and t.GetNetname() == "USB_DM":
        s, e = t.GetStart(), t.GetEnd()
        ys = {round(s.y/1e6, 3), round(e.y/1e6, 3)}
        if 107.95 in ys:
            b.Remove(t); removed += 1

# 2) zombie stitch vias from the first attempt + duplicate VTREF via pair
zombie = [(104.0, 128.0), (110.7, 136.6)]
dup_seen = set()
for t in _all:
    if t.GetClass() != "PCB_VIA":
        continue
    p = t.GetPosition()
    xy = (round(p.x/1e6, 2), round(p.y/1e6, 2))
    if xy in zombie:
        b.Remove(t); removed += 1
    elif t.GetNetname() == "TGT_VTREF":
        near = [d for d in dup_seen if math.hypot(xy[0]-d[0], xy[1]-d[1]) < 0.8]
        if near:
            b.Remove(t); removed += 1
        dup_seen.add(xy)
print("removed:", removed)

def track(net, layer, x1, y1, x2, y2, w=0.15):
    t = pcbnew.PCB_TRACK(b)
    t.SetStart(P(x1, y1)); t.SetEnd(P(x2, y2))
    t.SetWidth(mm(w)); t.SetLayer(layer); t.SetNet(net)
    b.Add(t)

def via(net, x, y):
    v = pcbnew.PCB_VIA(b)
    v.SetPosition(P(x, y))
    v.SetDrill(mm(0.3)); v.SetWidth(mm(0.6)); v.SetNet(net)
    v.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
    b.Add(v)

# 3) DM pair jumper: up past the corridor, across on B.Cu
track(dm, pcbnew.F_Cu, 107.75, 109.005, 107.75, 107.5)
via(dm, 107.75, 107.5)
track(dm, pcbnew.B_Cu, 107.75, 107.5, 108.75, 107.5)
via(dm, 108.75, 107.5)
track(dm, pcbnew.F_Cu, 108.75, 107.5, 108.75, 109.005)

# 4) J1 GND point pads -> shell PTH pads
track(gnd, pcbnew.F_Cu, 104.8, 109.005, 103.67, 108.245)
track(gnd, pcbnew.F_Cu, 111.2, 109.005, 112.33, 108.245)

# 5) zones: solid pad connection
for z in b.Zones():
    z.SetPadConnection(pcbnew.ZONE_CONNECTION_FULL)

filler = pcbnew.ZONE_FILLER(b)
filler.Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
