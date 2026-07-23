#!/usr/bin/env python3
"""Hand-route the SuperSpeed pairs (and the USB2 D+/- paths that thread the SS
fanout) on dongle-pro.kicad_pcb. Run AFTER gen/pcb-usb3.py, with KiCad's
bundled python. Deletes all existing tracks/vias first (idempotent).

Layer discipline: SS on F.Cu; polarity crossunders + B-row connector escapes on
In2 (GND1 above as reference) or B.Cu. Vias 0.45/0.2 except where 0.4mm QFN
pitch forces 0.3/0.15. Freerouting handles all remaining Default-class nets.
"""
import sys, os, types, re
os.chdir(os.path.join(os.path.dirname(__file__), '..'))
import pcbnew

W_SS = 0.211   # 90R diff on JLC04161H-7628 outer layers
W_DEF = 0.12
board = pcbnew.LoadBoard('dongle-pro.kicad_pcb')
LAYER = {'F': pcbnew.F_Cu, 'I2': pcbnew.In2_Cu, 'B': pcbnew.B_Cu}

pads = {}
for fp in board.GetFootprints():
    for pad in fp.Pads():
        pads[(fp.GetReference(), pad.GetNumber())] = pad

for t in list(board.GetTracks()):
    board.Remove(t)

def P(ref, num):
    p = pads[(ref, num)].GetPosition()
    return (p.x / 1e6, p.y / 1e6)

nets = {n.GetNetname(): n for n in board.GetNetInfo().NetsByName().values()}

def seg(net, layer, a, b, w=W_SS):
    t = pcbnew.PCB_TRACK(board)
    t.SetStart(pcbnew.VECTOR2I_MM(*a)); t.SetEnd(pcbnew.VECTOR2I_MM(*b))
    t.SetLayer(LAYER[layer]); t.SetWidth(pcbnew.FromMM(w))
    t.SetNet(nets[net]); board.Add(t)

def path(net, layer, pts, w=W_SS):
    for a, b in zip(pts, pts[1:]):
        if a != b: seg(net, layer, a, b, w)

def via(net, xy, d=0.45, drill=0.2):
    v = pcbnew.PCB_VIA(board)
    v.SetPosition(pcbnew.VECTOR2I_MM(*xy))
    v.SetWidth(pcbnew.FromMM(d)); v.SetDrill(pcbnew.FromMM(drill))
    v.SetViaType(pcbnew.VIATYPE_THROUGH)
    v.SetNet(nets[net]); board.Add(v)

def thru(ref, a, b, net):
    seg(net, 'F', P(ref, a), P(ref, b))

# ============================ HOST SECTION ============================
# TX1: J1(A3 N, A2 P) -> D12 ch1 -> caps -> U3 (P crossunder at U3 end)
path('HOST_TX1_N', 'F', [P('J1','A3'), (110.90,108.3), P('D12','10')])
thru('D12','10','1','HOST_TX1_N')
path('HOST_TX1_N', 'F', [P('D12','1'), P('C32','2')])
path('HOST_TX1_P', 'F', [P('J1','A2'), (111.40,108.3), P('D12','9')])
thru('D12','9','2','HOST_TX1_P')
path('HOST_TX1_P', 'F', [P('D12','2'), P('C31','2')])
path('M1_TX1_N', 'F', [P('C32','1'), (110.80,117.6), P('U3','16')])
via('M1_TX1_P', (111.90,117.5))
path('M1_TX1_P', 'F', [P('C31','1'), (111.90,117.5)])
path('M1_TX1_P', 'I2', [(111.90,117.5), (110.40,118.55)])
via('M1_TX1_P', (110.40,118.55), d=0.3, drill=0.15)
path('M1_TX1_P', 'F', [(110.40,118.55), P('U3','17')])
# RX1: B-row In2 escape (dodging shell PTH + elongated pads)
via('HOST_RX1_N', (110.95,104.40)); via('HOST_RX1_P', (111.50,104.20))
path('HOST_RX1_N', 'F', [P('J1','B10'), (110.95,104.40)])
path('HOST_RX1_P', 'F', [P('J1','B11'), (111.50,104.20)])
path('HOST_RX1_N', 'I2', [(110.95,104.40), (111.55,105.3), (111.55,106.9), (112.40,108.0), (112.40,110.6)])
path('HOST_RX1_P', 'I2', [(111.50,104.20), (111.95,105.0), (111.95,106.4), (112.90,107.25), (112.90,111.0)])
via('HOST_RX1_N', (112.40,110.6)); via('HOST_RX1_P', (112.90,111.0))
path('HOST_RX1_N', 'F', [(112.40,110.6), P('D12','7')])
thru('D12','7','4','HOST_RX1_N')
path('HOST_RX1_P', 'F', [(112.90,111.0), P('D12','6')])
thru('D12','6','5','HOST_RX1_P')
path('HOST_RX1_N', 'F', [P('D12','4'), (112.55,113.3), (112.55,121.10), P('U3','14')])
via('HOST_RX1_P', (113.05,117.6))
path('HOST_RX1_P', 'F', [P('D12','5'), (113.05,113.3), (113.05,117.6)])
path('HOST_RX1_P', 'I2', [(113.05,117.6), (111.90,119.9)])
via('HOST_RX1_P', (111.90,119.9))
path('HOST_RX1_P', 'F', [(111.90,119.9), P('U3','15')])
# TX2: B-row In2 escape -> D13 -> caps -> U3 (no inversion)
via('HOST_TX2_P', (106.90,104.30)); via('HOST_TX2_N', (107.45,104.55))
path('HOST_TX2_P', 'F', [P('J1','B2'), (106.90,104.30)])
path('HOST_TX2_N', 'F', [P('J1','B3'), (107.45,104.55)])
path('HOST_TX2_P', 'I2', [(106.90,104.30), (106.35,105.6), (106.15,107.0), (106.00,108.5), (106.00,110.9)])
path('HOST_TX2_N', 'I2', [(107.45,104.55), (106.85,105.9), (106.50,107.5), (106.50,110.55)])
via('HOST_TX2_P', (106.00,110.9)); via('HOST_TX2_N', (106.50,110.55))
path('HOST_TX2_P', 'F', [(106.00,110.9), P('D13','10')])
thru('D13','10','1','HOST_TX2_P')
path('HOST_TX2_N', 'F', [(106.50,110.55), P('D13','9')])
thru('D13','9','2','HOST_TX2_N')
path('HOST_TX2_P', 'F', [P('D13','1'), P('C33','2')])
path('HOST_TX2_N', 'F', [P('D13','2'), P('C34','2')])
path('M1_TX2_P', 'F', [P('C33','1'), (108.80,119.2), P('U3','21')])
path('M1_TX2_N', 'F', [P('C34','1'), (109.20,118.6), P('U3','20')])
# RX2: all F.Cu
path('HOST_RX2_P', 'F', [P('J1','A11'), (106.63,107.6), (107.50,109.4), P('D13','7')])
thru('D13','7','4','HOST_RX2_P')
path('HOST_RX2_N', 'F', [P('J1','A10'), (107.12,107.6), (108.00,109.4), P('D13','6')])
thru('D13','6','5','HOST_RX2_N')
path('HOST_RX2_P', 'F', [P('D13','4'), (109.60,117.8), P('U3','19')])
path('HOST_RX2_N', 'F', [P('D13','5'), (110.00,117.8), P('U3','18')])

# ================== HOST D+/D- (USB2, D10 on the left via B.Cu) ==================
# A6/B6 offset in x, so row ties split: P on In2, N on B.Cu. D10 channels are
# swapped in the netlist (1/6=N, 3/4=P) so the B.Cu diagonals never cross.
via('HOST_D_P', (108.88,104.45)); via('HOST_D_N', (109.42,104.28))
path('HOST_D_P', 'F', [P('J1','B6'), (108.88,104.45)], W_DEF)
path('HOST_D_N', 'F', [P('J1','B7'), (109.42,104.28)], W_DEF)
via('HOST_D_P', (109.3,108.4)); via('HOST_D_N', (108.63,108.35))
path('HOST_D_P', 'F', [P('J1','A6'), (109.12,107.3), (109.3,108.4)], W_DEF)
path('HOST_D_N', 'F', [P('J1','A7'), (108.63,108.35)], W_DEF)
path('HOST_D_P', 'I2', [(108.88,104.45), (109.3,108.4)], W_DEF)
path('HOST_D_N', 'B', [(109.42,104.28), (108.63,108.35)], W_DEF)
path('HOST_D_P', 'B', [(109.3,108.4), (107.2,111.6), (104.45,112.9)], W_DEF)
path('HOST_D_N', 'B', [(108.63,108.35), (102.55,112.9)], W_DEF)
via('HOST_D_P', (104.45,112.9)); via('HOST_D_N', (102.55,112.9))
path('HOST_D_P', 'F', [(104.45,112.9), P('D10','4')], W_DEF)
path('HOST_D_N', 'F', [(102.55,112.9), P('D10','6')], W_DEF)
via('HOST_D_P', (104.45,118.1)); via('HOST_D_N', (102.55,118.1))
path('HOST_D_P', 'F', [P('D10','3'), (104.45,118.1)], W_DEF)
path('HOST_D_N', 'F', [P('D10','1'), (102.55,118.1)], W_DEF)
path('HOST_D_P', 'B', [(104.45,118.1), (110.48,130.5)], W_DEF)
via('HOST_D_P', (110.48,130.5))
path('HOST_D_P', 'F', [(110.48,130.5), (110.40,131.4), P('U4','37')], W_DEF)
path('HOST_D_N', 'I2', [(102.55,118.1), (103.6,131.0), (105.5,133.35), (110.20,133.35), (110.78,132.2)], W_DEF)
via('HOST_D_N', (110.78,132.2))
path('HOST_D_N', 'F', [(110.78,132.2), P('U4','36')], W_DEF)

# ============================ U3 <-> U4 ============================
path('HUB_UTX_P', 'F', [P('U3','6'), (109.60,131.5), P('U4','39')])
path('HUB_UTX_N', 'F', [P('U3','7'), (110.00,131.5), P('U4','38')])
via('HUB_URX_P', (110.30,123.5), d=0.3, drill=0.15); via('HUB_URX_N', (110.80,124.3), d=0.3, drill=0.15)
path('HUB_URX_P', 'F', [P('U3','9'), (110.30,123.5)])
path('HUB_URX_N', 'F', [P('U3','10'), (110.80,124.3)])
path('HUB_URX_P', 'I2', [(110.30,123.5), (108.40,132.1)])
path('HUB_URX_N', 'I2', [(110.80,124.3), (108.80,132.4)])
# 0.4mm pad pitch at U4: use 0.3/0.15 vias directly below the pad columns
via('HUB_URX_P', (108.40,132.1), d=0.3, drill=0.15)
via('HUB_URX_N', (108.80,132.4), d=0.3, drill=0.15)
path('HUB_URX_P', 'F', [(108.40,132.1), P('U4','42')])
path('HUB_URX_N', 'F', [(108.80,132.4), P('U4','41')])

# ============================ U4 DS2 -> caps -> U7 ============================
path('HUB_DTX_N', 'F', [P('U4','56'), (102.80,136.80), P('C36','1')])
path('HUB_DTX_P', 'F', [P('U4','57'), (103.45,137.20), P('C35','1')])
path('M2_A0_N', 'F', [P('C36','2'), (102.80,153.75), P('U7','4')])
path('M2_A0_P', 'F', [P('C35','2'), (103.45,153.25), P('U7','3')])
via('HUB_DRX_N', (103.95,138.00)); via('HUB_DRX_P', (104.40,139.00))
path('HUB_DRX_N', 'F', [P('U4','59'), (103.95,138.00)])
path('HUB_DRX_P', 'F', [P('U4','60'), (104.55,138.40), (104.40,139.00)])
path('HUB_DRX_N', 'I2', [(103.95,138.00), (101.70,140.5), (101.70,155.75)])
path('HUB_DRX_P', 'I2', [(104.40,139.00), (102.20,141.5), (102.20,155.25)])
via('HUB_DRX_N', (101.70,155.75)); via('HUB_DRX_P', (102.20,155.25))
path('HUB_DRX_N', 'F', [(101.70,155.75), P('U7','8')])
path('HUB_DRX_P', 'F', [(102.20,155.25), P('U7','7')])

# ============================ U7 -> TVS -> J2 ============================
# lane2 (upper pads, B ch = J2 flipped lane): verticals to D15
path('TGT_TX2_P', 'F', [P('U7','19'), (111.40,152.75), P('D15','6')])
thru('D15','6','5','TGT_TX2_P')
path('TGT_TX2_N', 'F', [P('U7','18'), (110.90,153.25), P('D15','7')])
thru('D15','7','4','TGT_TX2_N')
path('TGT_RX2_P', 'F', [P('U7','17'), (109.90,153.75), P('D15','9')])
thru('D15','9','2','TGT_RX2_P')
path('TGT_RX2_N', 'F', [P('U7','16'), (109.40,154.25), P('D15','10')])
thru('D15','10','1','TGT_RX2_N')
# lane1 (lower pads, C ch = J2 normal lane): nested exits, synchronized diagonals
path('TGT_TX1_P', 'F', [P('U7','15'), (109.05,154.75), (109.05,158.0), (107.60,160.5), P('D14','6')])
thru('D14','6','5','TGT_TX1_P')
path('TGT_TX1_N', 'F', [P('U7','14'), (108.55,155.25), (108.55,158.0), (107.10,160.5), P('D14','7')])
thru('D14','7','4','TGT_TX1_N')
path('TGT_RX1_P', 'F', [P('U7','13'), (108.05,155.75), (108.05,158.0), (106.10,160.5), P('D14','9')])
thru('D14','9','2','TGT_RX1_P')
path('TGT_RX1_N', 'F', [P('U7','12'), (107.55,156.25), (107.55,158.0), (105.60,160.5), P('D14','10')])
thru('D14','10','1','TGT_RX1_N')
# below TVS -> J2. lane2 RX2 straight; TX2 (B-row) In2 escapes.
path('TGT_RX2_N', 'F', [P('D15','1'), (109.40,185.2), (110.88,188.6), P('J2','A10')])
path('TGT_RX2_P', 'F', [P('D15','2'), (109.90,185.2), (111.37,188.6), P('J2','A11')])
via('TGT_TX2_N', (110.90,185.35)); via('TGT_TX2_P', (111.55,184.9))
path('TGT_TX2_N', 'F', [P('D15','4'), (110.90,185.35)])
path('TGT_TX2_P', 'F', [P('D15','5'), (111.55,184.9)])
path('TGT_TX2_N', 'I2', [(110.90,185.35), (110.62,191.5)])
path('TGT_TX2_P', 'I2', [(111.55,184.9), (111.12,191.9)])
via('TGT_TX2_N', (110.62,191.5)); via('TGT_TX2_P', (111.12,191.9))
path('TGT_TX2_N', 'F', [(110.62,191.5), P('J2','B3')])
path('TGT_TX2_P', 'F', [(111.12,191.9), P('J2','B2')])
# lane1: TX1_N straight F; TX1_P B.Cu crossunder; RX1 In2/B.Cu B-row escapes.
path('TGT_TX1_N', 'F', [P('D14','4'), (107.37,188.9), P('J2','A3')])
via('TGT_TX1_P', (107.75,185.6))
path('TGT_TX1_P', 'F', [P('D14','5'), (107.75,185.6)])
path('TGT_TX1_P', 'B', [(107.75,185.6), (106.85,188.35)])
via('TGT_TX1_P', (106.85,188.35))
path('TGT_TX1_P', 'F', [(106.85,188.35), P('J2','A2')])
via('TGT_RX1_P', (106.10,185.35)); via('TGT_RX1_N', (105.60,184.95))
path('TGT_RX1_P', 'F', [(106.10,185.35), P('D14','2')])
path('TGT_RX1_N', 'F', [(105.60,184.95), P('D14','1')])
path('TGT_RX1_P', 'I2', [(106.10,185.35), (106.62,191.5)])
path('TGT_RX1_N', 'B', [(105.60,184.95), (105.60,186.2), (106.3,189.6), (107.25,190.9), (107.12,191.9)])
via('TGT_RX1_P', (106.62,191.5)); via('TGT_RX1_N', (107.12,191.9))
path('TGT_RX1_P', 'F', [(106.62,191.5), P('J2','B11')])
path('TGT_RX1_N', 'F', [(107.12,191.9), P('J2','B10')])

# ====================== TARGET D+/D- (USB2, D11 left of lane 1) ======================
# J2 center pads escape up via a "high road" above the TVS row (P on In2, N on
# B.Cu), into D11 at (103.6,186.5); from D11 down the west edge on B.Cu, then
# up to U4 DS2 USB2 pads 63/64. B6/B7 row ties: freerouting.
via('TGT_D_P', (108.88,188.45)); via('TGT_D_N', (109.37,188.10))
path('TGT_D_P', 'F', [P('J2','A6'), (108.88,188.45)], W_DEF)
path('TGT_D_N', 'F', [P('J2','A7'), (109.37,188.10)], W_DEF)
path('TGT_D_P', 'I2', [(108.88,188.45), (108.88,185.6), (108.6,183.0), (105.0,182.9), (103.1,184.9)], W_DEF)
via('TGT_D_P', (103.1,184.9))
path('TGT_D_P', 'F', [(103.1,184.9), P('D11','6')], W_DEF)
path('TGT_D_N', 'B', [(109.37,188.10), (108.75,183.1), (105.3,183.0), (104.9,185.0)], W_DEF)
via('TGT_D_N', (104.9,185.0))
path('TGT_D_N', 'F', [(104.9,185.0), P('D11','4')], W_DEF)
# D11 -> west lanes -> U4
via('TGT_D_P', (102.0,190.9)); via('TGT_D_N', (105.5,188.5))
path('TGT_D_P', 'F', [P('D11','1'), (102.0,188.5), (101.85,189.05), (101.85,190.4), (102.0,190.9)], W_DEF)
path('TGT_D_N', 'F', [P('D11','3'), (105.5,188.5)], W_DEF)
path('TGT_D_P', 'B', [(102.0,190.9), (99.7,188.3), (99.7,139.60), (104.75,139.60)], W_DEF)
path('TGT_D_N', 'B', [(105.5,188.5), (99.95,187.0), (99.95,140.00), (104.75,140.00)], W_DEF)
via('TGT_D_P', (104.75,139.60), d=0.3, drill=0.15)
via('TGT_D_N', (104.75,140.00), d=0.3, drill=0.15)
path('TGT_D_P', 'F', [(104.75,139.60), P('U4','63')], W_DEF)
path('TGT_D_N', 'F', [(104.75,140.00), P('U4','64')], W_DEF)

pcbnew.SaveBoard('dongle-pro.kicad_pcb', board)
_pro_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py')
pro = {'__file__': _pro_path}
exec(compile(open(_pro_path).read(), 'project.py', 'exec'), pro)
pro['write_pro']()
print(f"routed: {len(list(board.GetTracks()))} track/via items")
