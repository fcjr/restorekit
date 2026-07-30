#!/usr/bin/env python3
"""Rules reset + GND artifact purge + corrected VBUS routing. KiCad python."""
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

# rules (pcb layout regen resets these)
bds = b.GetDesignSettings()
nc = bds.m_NetSettings.GetDefaultNetclass()
nc.SetTrackWidth(mm(0.15))
nc.SetClearance(mm(0.13))
nc.SetViaDiameter(mm(0.6))
nc.SetViaDrill(mm(0.3))
bds.m_TrackMinWidth = mm(0.15)
bds.m_MinClearance = mm(0.1)
bds.m_ViasMinSize = mm(0.5)
bds.m_MinThroughDrill = mm(0.3)
bds.m_CopperEdgeClearance = mm(0.2)
bds.m_HoleToHoleMin = mm(0.25)

_all = list(b.GetTracks())
removed = 0
for t in _all:
    net = t.GetNetname()
    if net == "GND":
        b.Remove(t); removed += 1
        continue
    if net == "VBUS":
        if t.GetClass() == "PCB_VIA":
            pts = [(t.GetPosition().x / 1e6, t.GetPosition().y / 1e6)]
        else:
            s, e = t.GetStart(), t.GetEnd()
            pts = [(s.x / 1e6, s.y / 1e6), (e.x / 1e6, e.y / 1e6)]
        if all(y <= 111.6 and x >= 104.0 for x, y in pts):
            b.Remove(t); removed += 1
print("removed:", removed)

u2 = b.FindFootprintByReference("U2")
vpads = sorted([(p.GetPosition().y / 1e6, p.GetPosition().x / 1e6)
                for p in u2.Pads() if p.GetNetname() == "VBUS"])
uy, ux = vpads[0]
print("U2 VBUS target pad:", ux, uy)

def track(netname, pts, w=0.15):
    net = _nets[netname]
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(P(x1, y1)); t.SetEnd(P(x2, y2))
        t.SetWidth(mm(w)); t.SetLayer(pcbnew.F_Cu); t.SetNet(net)
        b.Add(t)

# B4A9 blade down and left into the LDO input
track("VBUS", [(105.6, 109.005), (105.6, 109.9), (ux, 109.9), (ux, uy)], w=0.3)
# D3 pad 5 west lane joining the feed
track("VBUS", [(108.0, 110.541), (108.0, 111.3), (105.0, 111.3), (105.0, 109.9)])
# A4B9 blade over the top of the pin row, down to the B4A9 blade
track("VBUS", [(110.4, 109.005), (110.2, 108.6), (110.2, 107.15), (105.75, 107.15),
               (105.75, 108.7), (105.6, 109.005)])

pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
