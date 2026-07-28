#!/usr/bin/env python3
"""Post-autoroute patch: close the last ~20 connections freerouting leaves open.
Runs AFTER gen/ses.py, BEFORE gen/tune.py. Only adds copper.
Run with KiCad's bundled python.
"""
import os
os.chdir(os.path.join(os.path.dirname(__file__), '..'))
import pcbnew

PCB = 'dongle-pro-power.kicad_pcb'
board = pcbnew.LoadBoard(PCB)
LAYER = {'F': pcbnew.F_Cu, 'I2': pcbnew.In2_Cu, 'B': pcbnew.B_Cu}
nets = {n.GetNetname(): n for n in board.GetNetInfo().NetsByName().values()}
V3 = dict(d=0.3, drill=0.15)


def seg(net, layer, a, b, w=0.15):
    t = pcbnew.PCB_TRACK(board)
    t.SetStart(pcbnew.VECTOR2I_MM(*a)); t.SetEnd(pcbnew.VECTOR2I_MM(*b))
    t.SetLayer(LAYER[layer]); t.SetWidth(pcbnew.FromMM(w))
    t.SetNet(nets[net]); board.Add(t)


def path(net, layer, pts, w=0.15):
    for a, b in zip(pts, pts[1:]):
        if a != b:
            seg(net, layer, a, b, w)


def via(net, xy, d=0.45, drill=0.2):
    v = pcbnew.PCB_VIA(board)
    v.SetPosition(pcbnew.VECTOR2I_MM(*xy))
    v.SetWidth(pcbnew.FromMM(d)); v.SetDrill(pcbnew.FromMM(drill))
    v.SetViaType(pcbnew.VIATYPE_THROUGH)
    v.SetNet(nets[net]); board.Add(v)


# --- V_SENSE: U1.43 -> divider node (north of the HUB_RSTn via)
path('V_SENSE', 'F', [(119.78, 163.00), (120.6, 163.0), (121.8, 164.2), (122.30, 165.07)], 0.15)
# --- I_SENSE: U1.42 -> U15.1 (east) on Sig2 (crosses HOST_VBUS B harmlessly)
via('I_SENSE', (119.78, 163.40), **V3); via('I_SENSE', (128.10, 176.35), **V3)
path('I_SENSE', 'I2', [(119.78, 163.40), (121.0, 165.0), (124.0, 171.0), (127.0, 175.5),
                       (128.10, 176.35)], 0.2)
# --- VBUS_DISCHG: U1.18 -> Q5.4 (east) on Sig2
via('VBUS_DISCHG', (114.20, 168.57), **V3); via('VBUS_DISCHG', (132.59, 177.70), **V3)
path('VBUS_DISCHG', 'I2', [(114.20, 168.57), (116.0, 172.5), (124.0, 174.0), (130.0, 177.0),
                           (132.59, 177.70)], 0.2)
# --- SRC20: U12.9 stub
path('SRC20', 'F', [(132.72, 165.99), (133.5, 164.0), (134.35, 162.0), (134.35, 161.19)], 0.3)
# --- VSAFE_SRC: U13.2 -> Q3 vSafe track
path('VSAFE_SRC', 'F', [(133.35, 190.61), (132.0, 189.0), (128.0, 185.5), (126.5, 184.3),
                        (126.01, 184.14)], 0.3)
# --- PWR_CC1: J4.A5 -> U11 stub, B around J4 NE (clear of STUSB_DIS Sig2)
via('PWR_CC1', (131.49, 147.37), **V3); via('PWR_CC1', (126.75, 138.4), **V3)
path('PWR_CC1', 'B', [(131.49, 147.37), (132.4, 143.0), (132.4, 141.0), (127.5, 141.0),
                      (126.75, 138.4)], 0.25)
path('PWR_CC1', 'F', [(126.75, 138.4), (126.75, 137.0)], 0.25)
# --- HUB_VBUS_SNS: U4.25 -> its via, south of U4.26
path('HUB_VBUS_SNS', 'F', [(112.85, 136.80), (114.0, 137.2), (115.5, 136.4), (116.17, 135.78)], 0.15)
# --- +5V: D11.5 -> R25 track; R17 -> +5V trunk
path('+5V', 'F', [(103.60, 185.85), (108.0, 184.0), (112.0, 182.6), (113.80, 182.43)], 0.3)
via('+5V', (99.00, 134.25), **V3); via('+5V', (100.20, 122.00), **V3)
path('+5V', 'B', [(99.00, 134.25), (99.5, 128.0), (100.2, 123.0), (100.20, 122.00)], 0.3)
# --- +1V2: U6.5 -> track (land the via clear of SBU1_UART Sig2)
via('+1V2', (112.85, 187.45), **V3); via('+1V2', (104.60, 183.60), **V3)
path('+1V2', 'B', [(112.85, 187.45), (110.0, 185.2), (105.3, 184.0), (104.60, 183.60)], 0.2)
path('+1V2', 'F', [(104.60, 183.60), (104.11, 182.99)], 0.2)
# --- TGT_SBU2: U9.4 -> J2 track on B
via('TGT_SBU2', (102.55, 182.25), **V3); via('TGT_SBU2', (108.12, 190.61), **V3)
path('TGT_SBU2', 'B', [(102.55, 182.25), (103.4, 183.4), (103.4, 189.5), (107.2, 190.61),
                       (108.12, 190.61)], 0.2)
# --- +3V3 fragments (short local closes)
path('+3V3', 'F', [(100.99, 159.61), (102.5, 157.0), (104.10, 154.58)], 0.3)
via('+3V3', (112.98, 176.90), **V3); via('+3V3', (103.50, 175.92), **V3)
path('+3V3', 'B', [(112.98, 176.90), (108.0, 177.4), (104.0, 176.4), (103.50, 175.92)], 0.3)
path('+3V3', 'F', [(118.88, 177.20), (117.9, 176.1), (116.98, 175.11)], 0.3)
via('+3V3', (116.98, 174.60), **V3)
path('+3V3', 'B', [(116.98, 174.60), (117.8, 170.5), (119.00, 166.50)], 0.3)

pcbnew.SaveBoard(PCB, board)
_pro = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py')
p = {'__file__': _pro}
exec(compile(open(_pro).read(), 'project.py', 'exec'), p)
p['write_pro']()
print('patch corridors applied')
