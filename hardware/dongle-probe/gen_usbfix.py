#!/usr/bin/env python3
"""Rip and hand-route the USB-C pin-row region; swap CC pulldowns; island vias; refill."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()
gnd = _nets["GND"]

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

# 1) rip CC nets entirely; rip DP/DM copper above y=110.3
_all = list(b.GetTracks())
removed = 0
for t in _all:
    net = t.GetNetname()
    if net in ("CC1", "CC2"):
        b.Remove(t); removed += 1
    elif net in ("USB_DP", "USB_DM"):
        if t.GetClass() == "PCB_VIA":
            if t.GetPosition().y / 1e6 < 110.3:
                b.Remove(t); removed += 1
        else:
            if t.GetStart().y / 1e6 < 110.3 and t.GetEnd().y / 1e6 < 110.3:
                b.Remove(t); removed += 1
print("ripped:", removed)

# 2) swap the CC pulldowns so each sits on its pad's side
r1 = b.FindFootprintByReference("R1")
r2 = b.FindFootprintByReference("R2")
p1, p2 = r1.GetPosition(), r2.GetPosition()
r1.SetPosition(pcbnew.VECTOR2I(p2.x, p1.y))
r2.SetPosition(pcbnew.VECTOR2I(p1.x, p2.y))

def pad_pos(fp, netname):
    for p in fp.Pads():
        if p.GetNetname() == netname:
            q = p.GetPosition()
            return q.x / 1e6, q.y / 1e6
    raise KeyError(netname)

def track(netname, layer, pts, w=0.15):
    net = _nets[netname]
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(P(x1, y1)); t.SetEnd(P(x2, y2))
        t.SetWidth(mm(w)); t.SetLayer(layer); t.SetNet(net)
        b.Add(t)

# 3) hand routes
# DP pair join through the (now empty) corridor above the pin row
track("USB_DP", pcbnew.F_Cu, [(108.25, 109.005), (108.25, 107.6), (107.25, 107.6), (107.25, 109.005)])
# B6 down into D3 pad 6
track("USB_DP", pcbnew.F_Cu, [(107.25, 109.005), (107.05, 109.9), (107.05, 110.541)])
# DM: A7 and B7 down into D3 pad 4
track("USB_DM", pcbnew.F_Cu, [(107.75, 109.005), (107.75, 109.75), (108.95, 109.75), (108.95, 110.541)])
track("USB_DM", pcbnew.F_Cu, [(108.75, 109.005), (108.75, 109.75)])
# CC nets to the swapped resistors
c1x, c1y = pad_pos(r1, "CC1")
c2x, c2y = pad_pos(r2, "CC2")
track("CC1", pcbnew.F_Cu, [(109.25, 109.005), (109.25, c1y), (c1x, c1y)])
track("CC2", pcbnew.F_Cu, [(106.25, 109.005), (106.25, c2y), (c2x, c2y)])

# 4) refill and save; island pass runs as a second phase
filler = pcbnew.ZONE_FILLER(b)
filler.Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("phaseA saved")
