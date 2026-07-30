#!/usr/bin/env python3
"""Move J1 flush with the board edge (dy=-1.445, same offset as dongle-lite)
and shift the connector-side routing with it. Points at or north of the J1
pin row (y<=109.02) translate; anchored south ends stay. KiCad python."""
import math
import pcbnew

BOARD = "layout/layout.kicad_pcb"
DY = -1.445
YCUT = 109.02
NETS = ("VBUS", "USB_DP", "USB_DM", "CC1", "CC2")
GND_TIES = [((104.8, 109.005), (103.67, 108.245)),
            ((111.2, 109.005), (112.33, 108.245))]

b = pcbnew.LoadBoard(BOARD)

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

def near(a, bpt, tol=0.03):
    return math.hypot(a[0] - bpt[0], a[1] - bpt[1]) < tol

for fp in b.GetFootprints():
    if fp.GetReference() == "J1":
        p = fp.GetPosition()
        fp.SetPosition(pcbnew.VECTOR2I(p.x, p.y + mm(DY)))
        print("J1 ->", fp.GetPosition().y / 1e6)

moved = 0
items = []
for t in b.GetTracks():
    if t.GetClass() == "PCB_VIA":
        p = t.GetPosition()
        items.append((t, "via", (p.x/1e6, p.y/1e6), (p.x/1e6, p.y/1e6)))
    else:
        s, e = t.GetStart(), t.GetEnd()
        items.append((t, "trk", (s.x/1e6, s.y/1e6), (e.x/1e6, e.y/1e6)))

for obj, kind, s, e in items:
    net = obj.GetNetname()
    shift_s = shift_e = False
    if net in NETS:
        shift_s = s[1] <= YCUT
        shift_e = e[1] <= YCUT
    elif net == "GND":
        for a, c in GND_TIES:
            if (near(s, a) and near(e, c)) or (near(s, c) and near(e, a)):
                shift_s = shift_e = True
    if not (shift_s or shift_e):
        continue
    if kind == "via":
        if shift_s:
            obj.SetPosition(P(s[0], s[1] + DY))
            moved += 1
    else:
        if shift_s:
            obj.SetStart(P(s[0], s[1] + DY))
        if shift_e:
            obj.SetEnd(P(e[0], e[1] + DY))
        moved += 1
print("moved:", moved)

pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
