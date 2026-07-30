#!/usr/bin/env python3
"""Final USB-C region architecture: DP hops via B.Cu, DM joins above, VBUS outer lane."""
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

_all = list(b.GetTracks())
removed = 0
for t in _all:
    net = t.GetNetname()
    if t.GetClass() == "PCB_VIA":
        pts = [(t.GetPosition().x / 1e6, t.GetPosition().y / 1e6)]
    else:
        s, e = t.GetStart(), t.GetEnd()
        pts = [(s.x / 1e6, s.y / 1e6), (e.x / 1e6, e.y / 1e6)]
    rip = False
    if net in ("USB_DP", "USB_DM") and all(y <= 111.9 for _, y in pts):
        rip = True
    elif net == "VBUS" and all(y <= 111.6 and x >= 104.0 for x, y in pts):
        rip = True
    elif net == "VBUS" and any(y > 111.0 and x < 104.2 for x, y in pts) and all(y <= 112.6 for _, y in pts):
        rip = True
    if rip:
        b.Remove(t)
        removed += 1
print("ripped:", removed)

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

# DP: B6 and A6 stub up to vias, join on B.Cu (passes under A7's F.Cu ascent)
track("USB_DP", F, [(107.25, 109.005), (107.25, 108.1), (107.2, 107.9)])
via("USB_DP", 107.2, 107.9)
track("USB_DP", F, [(108.25, 109.005), (108.25, 108.1), (108.3, 107.9)])
via("USB_DP", 108.3, 107.9)
track("USB_DP", B, [(107.2, 107.9), (108.3, 107.9)])
# B6 down into D3 pad 6, and pad6 tied to pad1 (flow-through)
track("USB_DP", F, [(107.25, 109.005), (107.25, 109.86), (107.05, 110.2), (107.05, 110.541)])
track("USB_DP", F, [(107.05, 110.541), (107.05, 112.841)])
# DM: A7 ascends, joins B7 on a lane above the row
track("USB_DM", F, [(107.75, 109.005), (107.75, 107.15), (108.95, 107.15),
                    (108.95, 108.2), (108.75, 108.5), (108.75, 109.005)])
# B7 down into D3 pad 4
track("USB_DM", F, [(108.75, 109.005), (108.85, 109.35), (108.85, 110.1), (108.95, 110.45), (108.95, 110.541)])
# VBUS: A4B9 over the top to B4A9, B4A9 down-left into the LDO input
track("VBUS", F, [(110.4, 109.005), (110.2, 108.6), (110.2, 106.7), (105.75, 106.7),
                  (105.75, 108.7), (105.6, 109.005)])
track("VBUS", F, [(105.6, 109.005), (105.6, 109.9), (103.3545, 109.9), (103.3545, 110.5501)], w=0.3)
# D3 pad 5 hops to B.Cu, west, then up to the feed
via("VBUS", 108.0, 111.55)
track("VBUS", F, [(108.0, 110.541), (108.0, 111.55)])
track("VBUS", B, [(108.0, 111.55), (105.0, 111.55)])
via("VBUS", 105.0, 111.55)
track("VBUS", F, [(105.0, 111.55), (105.0, 109.9)])
# J1 GND blade points tied to the shell pads
track("GND", F, [(104.8, 109.005), (103.67, 108.245)])
track("GND", F, [(111.2, 109.005), (112.33, 108.245)])

pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
