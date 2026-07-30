#!/usr/bin/env python3
"""Deterministic USB-C region routing (rip + rebuild). KiCad python."""
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
        x, y = t.GetPosition().x / 1e6, t.GetPosition().y / 1e6
        pts = [(x, y)]
    else:
        s, e = t.GetStart(), t.GetEnd()
        pts = [(s.x / 1e6, s.y / 1e6), (e.x / 1e6, e.y / 1e6)]
    rip = False
    if net in ("CC1", "CC2"):
        rip = True
    elif net in ("USB_DP", "USB_DM") and all(y <= 111.5 for _, y in pts):
        rip = True
    elif net == "VBUS" and all(y <= 111.6 and x >= 104.0 for x, y in pts):
        rip = True
    if rip:
        b.Remove(t)
        removed += 1
print("ripped:", removed)

r1 = b.FindFootprintByReference("R1")
r2 = b.FindFootprintByReference("R2")

def pad_pos(fp, netname):
    for p in fp.Pads():
        if p.GetNetname() == netname:
            q = p.GetPosition()
            return q.x / 1e6, q.y / 1e6
    raise KeyError(netname)

# CC pad of R1 must face left (toward A5), CC pad of R2 must face right (toward B5)
if pad_pos(r1, "CC1")[0] > pad_pos(r1, "GND")[0]:
    r1.SetOrientationDegrees(r1.GetOrientationDegrees() + 180)
if pad_pos(r2, "CC2")[0] < pad_pos(r2, "GND")[0]:
    r2.SetOrientationDegrees(r2.GetOrientationDegrees() + 180)
c1x, c1y = pad_pos(r1, "CC1")
c2x, c2y = pad_pos(r2, "CC2")
u2 = b.FindFootprintByReference("U2")
uvx, uvy = pad_pos(u2, "VBUS")
print("R1 CC1 pad", c1x, c1y, "| R2 CC2 pad", c2x, c2y, "| U2 VBUS pad", uvx, uvy)

def track(netname, pts, w=0.15):
    net = _nets[netname]
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(P(x1, y1)); t.SetEnd(P(x2, y2))
        t.SetWidth(mm(w)); t.SetLayer(pcbnew.F_Cu); t.SetNet(net)
        b.Add(t)

# DP: A6 joins B6 through the corridor above the row; B6 drops into D3 pad 6
track("USB_DP", [(108.25, 109.005), (108.25, 107.6), (107.25, 107.6), (107.25, 109.005)])
track("USB_DP", [(107.25, 109.005), (107.25, 109.9), (107.05, 110.2), (107.05, 110.541)])
# DM: A7 + B7 drop to a bus at y=109.75, into D3 pad 4
track("USB_DM", [(107.75, 109.005), (107.75, 109.75), (108.95, 109.75), (108.95, 110.541)])
track("USB_DM", [(108.75, 109.005), (108.75, 109.75)])
# CC1: A5 down, jog right past D3 pad4, to R1's CC pad
track("CC1", [(109.25, 109.005), (109.25, 109.75), (109.5, 110.0), (109.5, c1y), (c1x, c1y)])
# CC2: B5 straight down to R2's CC pad row
track("CC2", [(106.25, 109.005), (106.25, c2y), (c2x, c2y)])
# VBUS: B4A9 down and left to the LDO input pad
track("VBUS", [(105.6, 109.005), (105.6, 109.9), (uvx, 109.9), (uvx, uvy)])
# VBUS: D3 pad 5 down, west lane, up to the B4A9 drop
track("VBUS", [(108.0, 110.541), (108.0, 111.3), (105.2, 111.3), (105.2, 109.9)])
# VBUS: A4B9 around R1 to the pad-5 lane
track("VBUS", [(110.4, 109.005), (110.4, 109.6), (111.6, 109.6), (111.6, 111.3), (108.0, 111.3)])

pcbnew.SaveBoard(BOARD, b)
print("saved")
