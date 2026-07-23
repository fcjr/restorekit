#!/usr/bin/env python3
"""Add GND zones on all four layers (In1 = the solid reference plane under the
SS corridor), fill them, and drop GND stitching vias near the SS crossunder
via clusters. Run with KiCad's bundled python after gen/ses-usb3.py.
"""
import os, sys
os.chdir(os.path.join(os.path.dirname(__file__), '..'))
import pcbnew

PCB = 'dongle-pro.kicad_pcb'
board = pcbnew.LoadBoard(PCB)
gnd = board.GetNetInfo().NetsByName()['GND']

X0, Y0, X1, Y1 = 96.0, 99.0, 122.0, 197.0

# wipe old zones (idempotent)
for z in list(board.Zones()):
    board.Remove(z)

def add_zone(layer, prio):
    z = pcbnew.ZONE(board)
    pts = [(X0, Y0), (X1, Y0), (X1, Y1), (X0, Y1)]
    chain = pcbnew.SHAPE_LINE_CHAIN()
    for x, y in pts:
        chain.Append(pcbnew.VECTOR2I_MM(x, y))
    chain.SetClosed(True)
    z.Outline().AddOutline(chain)
    z.SetLayer(layer)
    z.SetNet(gnd)
    z.SetAssignedPriority(prio)
    z.SetLocalClearance(pcbnew.FromMM(0.25))
    z.SetMinThickness(pcbnew.FromMM(0.25))
    z.SetThermalReliefGap(pcbnew.FromMM(0.5))
    z.SetThermalReliefSpokeWidth(pcbnew.FromMM(0.5))
    z.SetPadConnection(pcbnew.ZONE_CONNECTION_FULL)
    z.SetIsFilled(False)
    board.Add(z)
    return z

for layer in (pcbnew.F_Cu, pcbnew.In1_Cu, pcbnew.In2_Cu, pcbnew.B_Cu):
    add_zone(layer, 0)

# GND stitching vias: near SS crossunder via clusters + spread along corridors.
STITCH = [
    # host connector / RX escape region
    (110.30, 103.9), (112.05, 104.6), (113.3, 106.5), (105.6, 104.2),
    (105.45, 106.6), (108.20, 104.1),
    # TVS/cap region + U3 crossunders
    (113.5, 115.6), (109.9, 116.4), (104.9, 111.9), (112.5, 118.4),
    (110.9, 121.9), (108.2, 117.6),
    # U3<->U4 / URX crossunder
    (109.35, 124.2), (107.9, 130.9), (111.4, 128.4),
    # DS2 left bus + DRX crossunder
    (103.2, 139.9), (101.1, 142.9), (102.9, 150.9), (100.9, 152.4),
    (104.5, 143.3),
    # U7 fan / lane split
    (108.4, 151.3), (105.3, 157.9), (108.85, 157.4),
    # target corridor mid
    (106.3, 166.0), (108.3, 172.0), (106.6, 176.5), (109.9, 167.8),
    # target TVS / J2 escapes
    (105.1, 183.2), (108.35, 183.9), (112.55, 183.3), (109.95, 190.15),
    (105.9, 190.3), (112.1, 190.3),
]
def clear_of_everything(x, y, min_d=0.55):
    pt = pcbnew.VECTOR2I_MM(x, y)
    for t in board.GetTracks():
        if t.GetNetname() == 'GND':
            continue
        if t.HitTest(pt, pcbnew.FromMM(min_d)):
            return False
    for fp in board.GetFootprints():
        for pad in fp.Pads():
            if pad.GetNetname() == 'GND':
                continue
            if pad.HitTest(pt, pcbnew.FromMM(min_d)):
                return False
    return True

placed = skipped = 0
for x, y in STITCH:
    if not clear_of_everything(x, y):
        skipped += 1
        continue
    v = pcbnew.PCB_VIA(board)
    v.SetPosition(pcbnew.VECTOR2I_MM(x, y))
    v.SetWidth(pcbnew.FromMM(0.45)); v.SetDrill(pcbnew.FromMM(0.2))
    v.SetViaType(pcbnew.VIATYPE_THROUGH)
    v.SetNet(gnd)
    board.Add(v)
    placed += 1

filler = pcbnew.ZONE_FILLER(board)
filler.Fill(board.Zones())
pcbnew.SaveBoard(PCB, board)
_pro_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py')
pro = {'__file__': _pro_path}
exec(compile(open(_pro_path).read(), 'project.py', 'exec'), pro)
pro['write_pro']()
print(f'zones filled; stitch vias placed={placed} skipped={skipped}')
