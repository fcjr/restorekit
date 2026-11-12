#!/usr/bin/env python3
"""Route dongle-pro-v2-pd.kicad_pcb by hand. Run after gen/pcb.py with KiCad's
python. Idempotent: strips every track and via first.

Layer plan: F.Cu = components + the A-row SuperSpeed pairs and the mux-to-mux
corridor; In1/In2 = solid GND; B.Cu = the B-row (through-hole) SuperSpeed pairs
near each receptacle, the USB 2.0 trunk, and slow signals crossing the corridor.
Target VBUS leaves the mux on F.Cu, drops to B.Cu past the SBU2 run and reaches
all four VBUS contacts of the target port from underneath.
Every bend is an arc; pair legs bend on concentric arcs.
"""
import os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
os.chdir(os.path.join(HERE, '..'))
sys.path.insert(0, HERE)
from rl import Router
R = Router('dongle-pro-v2-pd.kicad_pcb')
P, T, V = R.P, R.track, R.via
W_SS = 0.211          # 90 ohm coupled on JLC04161H-7628 outer layers (with 0.127 gap)
CX = 110.5
# A through-hole-row pair turns 90 degrees into the mux's side pins, which leaves
# the inside leg about 0.8 mm short. It gets that back in two small bumps toward
# the empty ground-pin column of the TVS array (tuned with gen/audit.py).
BUMP_H = 0.54
def bumps(x, y, s, n=2):
    d = 1 if x < CX else -1; pts = [(x, y)]
    for _ in range(n):
        pts += [(x + d * BUMP_H, y + s * BUMP_H), (x, y + 2 * s * BUMP_H)]; y += 2 * s * BUMP_H
    return pts

# ---------------------------------------------------------------- SuperSpeed
def lane(J, yj, s, D, a, b, U):
    """One lane at one receptacle.
    a = (A-row pad L, A-row pad R, esd near L, esd far L, esd near R, esd far R, mux L, mux R)
    b = (B-row pin L, B-row pin R, esd near L, esd far L, esd near R, esd far R, mux L, mux R)
    'L/R' are just the two legs; `s` is +1 when signals leave the receptacle
    toward +y (host, top edge) and -1 at the target."""
    # A-row pair: all on F.Cu, both legs the same shape
    for pj, en, ef, pm in ((a[0], a[2], a[3], a[6]), (a[1], a[4], a[5], a[7])):
        net = R.netof(f'{J}.{pj}')
        xj, _ = P(f'{J}.{pj}'); xe, ye = P(f'{D}.{en}'); xm, ym = P(f'{U}.{pm}')
        T(net, 'F', [f'{J}.{pj}', (xj, yj + s * 4.2), (xe, yj + s * 5.6), f'{D}.{en}'], W_SS, r=0.9)
        T(net, 'F', [f'{D}.{en}', f'{D}.{ef}'], W_SS)
        T(net, 'F', [f'{D}.{ef}', (xe, yj + s * 8.4), (xm, yj + s * 9.7), f'{U}.{pm}'], W_SS, r=0.9)
    # B-row pair: B.Cu from the through-hole pins, staggered vias, then F.Cu
    xs = [P(f'{J}.{b[0]}')[0], P(f'{J}.{b[1]}')[0]]; mid = sum(xs) / 2
    outer_x = max((P(f'{D}.{b[2]}')[0], P(f'{D}.{b[4]}')[0]), key=lambda x: abs(x - CX))
    for pj, en, ef, pm in ((b[0], b[2], b[3], b[6]), (b[1], b[4], b[5], b[7])):
        net = R.netof(f'{J}.{pj}')
        xj, yp = P(f'{J}.{pj}'); xe, ye = P(f'{D}.{en}'); xm, ym = P(f'{U}.{pm}')
        xc = mid + (0.25 if xj > mid else -0.25)
        is_outer = abs(xe - outer_x) < 1e-3
        yv = yj + s * (5.5 if is_outer else 6.05)
        # both legs fan out at the same slope so the gap never closes; the fan
        # starts past the receptacle's locating-peg hole
        yd = 3.6 + abs(xe - xc) * 1.3
        T(net, 'B', [f'{J}.{pj}', (xc, yj + s * 1.5), (xc, yj + s * 3.6), (xe, yj + s * yd), (xe, yv)], W_SS, r=0.7)
        V(net, (xe, yv))
        T(net, 'F', [(xe, yv), f'{D}.{en}'], W_SS)
        T(net, 'F', [f'{D}.{en}', f'{D}.{ef}'], W_SS)
        # into the mux side pin: the leg further from the mux takes the larger arc
        yf = P(f'{D}.{ef}')[1]
        if is_outer: T(net, 'F', [f'{D}.{ef}', (xe, ym), f'{U}.{pm}'], W_SS, r=0.85)
        else:
            bp = bumps(xe, yf + s * 0.55, s)
            T(net, 'F', [f'{D}.{ef}'] + bp, W_SS, r=0.1); T(net, 'F', [bp[-1], (xe, ym), f'{U}.{pm}'], W_SS, r=0.35)

# host, lane 1 (right) and lane 2 (left)
lane('J1', 104.4, +1, 'D12', ('A2', 'A3', '9', '2', '10', '1', '1', '2'), ('B11', 'B10', '6', '5', '7', '4', '15', '16'), 'U3')
lane('J1', 104.4, +1, 'D13', ('A11', 'A10', '7', '4', '6', '5', '4', '3'), ('B2', 'B3', '10', '1', '9', '2', '6', '5'), 'U3')
# target: the lanes go mux -> TVS array -> 330nF -> bleed resistor -> receptacle.
# Mux side is the same shape as the host's. Below the arrays every line runs
# straight down through its cap (two staggered rows), then the A-row pair steps
# 0.15 mm onto its pads and the B-row pair steps 1 mm outward to its vias.
YJ = 150.6
def tlane(jpin, D, en, ef, mux, cap, res, xl, brow=None):
    """brow = (via y, B.Cu x next to the pin, is the outer leg) for a through-hole pin."""
    nic = R.netof(f'{D}.{en}'); ncon = R.netof(f'J2.{jpin}')
    xe, _ = P(f'{D}.{en}'); xm, ym = P(f'U4.{mux}')
    yf = P(f'{D}.{ef}')[1]
    if brow and brow[2]: T(nic, 'F', [f'{D}.{ef}', (xe, ym), f'U4.{mux}'], W_SS, r=0.85)
    elif brow:
        bp = bumps(xe, yf - 0.55, -1)
        T(nic, 'F', [f'{D}.{ef}'] + bp, W_SS, r=0.1); T(nic, 'F', [bp[-1], (xe, ym), f'U4.{mux}'], W_SS, r=0.35)
    else:    T(nic, 'F', [f'{D}.{ef}', (xe, 135.2), (xm, 133.9), f'U4.{mux}'], W_SS, r=0.9)
    T(nic, 'F', [f'{D}.{en}', f'{D}.{ef}'], W_SS); T(nic, 'F', [f'{D}.{en}', f'{cap}.2'], W_SS, r=0)
    down = [f'{cap}.1', (xe, 140.45), (xl, 140.45 + abs(xl - xe))]
    if brow:
        yv, xc, _ = brow
        T(ncon, 'F', down + [(xl, yv)], W_SS, r=0.5); V(ncon, (xl, yv))
        T(ncon, 'B', [f'J2.{jpin}', (xc, YJ - 1.5), (xc, YJ - 3.6), (xl, YJ - 3.6 - abs(xl - xc)), (xl, yv)], W_SS, r=0.7)
    else:
        T(ncon, 'F', down + [f'J2.{jpin}'], W_SS, r=0.3)
tlane('B11', 'D14', '1', '10', '15', 'C30', 'R60', 105.9,  (144.3,  108.25, True))
tlane('B10', 'D14', '2', '9',  '16', 'C31', 'R61', 106.4,  (143.75, 108.75, False))
tlane('A2',  'D14', '4', '7',  '1',  'C32', 'R62', 108.25)
tlane('A3',  'D14', '5', '6',  '2',  'C33', 'R63', 108.75)
tlane('A11', 'D15', '2', '9',  '4',  'C34', 'R64', 112.75)
tlane('A10', 'D15', '1', '10', '3',  'C35', 'R65', 112.25)
tlane('B2',  'D15', '5', '6',  '6',  'C36', 'R66', 115.1,  (144.3,  112.75, True))
tlane('B3',  'D15', '4', '7',  '5',  'C37', 'R67', 114.6,  (143.75, 112.25, False))

# mux-to-mux corridor: two coupled pairs straight down the middle on F.Cu
for (pl, pr, ql, qr), xc in ((('9', '10', '12', '11'), 110.0), (('11', '12', '10', '9'), 111.0)):
    for pu, pd, dx in ((pl, ql, -0.169), (pr, qr, +0.169)):
        net = R.netof(f'U3.{pu}'); xa, ya = P(f'U3.{pu}'); xb, yb = P(f'U4.{pd}')
        T(net, 'F', [f'U3.{pu}', (xa, ya + 0.55), (xc + dx, ya + 1.25), (xc + dx, yb - 1.25), (xb, yb - 0.55), f'U4.{pd}'], W_SS, r=1.2)

# ---------------------------------------------------------------- host port centre
W = 0.15; WP = 0.3
T('GND', 'F', ['J1.A1', 'J1.B12'], 0.25); T('GND', 'F', ['J1.A12', 'J1.B1'], 0.25)
T('+5V', 'F', ['J1.A4', 'J1.B9'], 0.25)
# A6/A7 drop to B.Cu in the channel between the two ESD arrays
V6H, V7H, VC1H = (110.75, 109.0), (110.25, 109.65), (111.3, 109.65)
T('HOST_D_P', 'F', ['J1.A6', V6H], 0.2); V('HOST_D_P', V6H)
T('HOST_D_N', 'F', ['J1.A7', V7H], 0.2); V('HOST_D_N', V7H)
T('HOST_CC1', 'F', ['J1.A5', (111.25, 109.0), VC1H], W); V('HOST_CC1', VC1H)
# the receptacle mirrors A6/A7 against B6/B7, so one net has to hop a layer:
# D- joins on B.Cu, D+ takes a 3 mm jumper on In2
T('HOST_D_N', 'B', [V7H, (110.25, 108.3), 'J1.B7'], 0.2, r=0.6)
T('HOST_D_P', 'I2', [V6H, (110.75, 108.2), 'J1.B6'], 0.2, r=0.6)
# trunk: down the centre under the mux, west, then south to the hub-side vias
VHP, VHN = (108.2, 118.75), (107.75, 119.5)
R.pair('HOST_D_P', 'HOST_D_N', 'B', [(110.5, 110.6), (110.5, 113.5), (107.95, 113.5), (107.95, 118.1)],
       w=0.2, gap=0.15, r=0.7, ends=((V6H, V7H), (VHP, VHN)))
V('HOST_D_P', VHP); V('HOST_D_N', VHN)
T('HOST_D_P', 'F', ['U5.12', 'D10.9']); T('HOST_D_P', 'F', ['D10.9', 'D10.2']); T('HOST_D_P', 'F', ['D10.2', VHP], 0.2)
T('HOST_D_N', 'F', ['U5.11', 'D10.10']); T('HOST_D_N', 'F', ['D10.10', 'D10.1']); T('HOST_D_N', 'F', ['D10.1', (107.5, 119.25), VHN], 0.2, r=0.3)
# CC1: B.Cu down beside the trunk, up next to the mux pin and its Rd
VC1B = (112.9, 117.3)
T('HOST_CC1', 'B', [VC1H, (111.3, 116.6), (112.0, 117.3), VC1B], W, r=0.8); V('HOST_CC1', VC1B)
T('HOST_CC1', 'F', ['U3.13', (112.55, 116.85), VC1B], W, r=0.3); T('HOST_CC1', 'F', [VC1B, 'R6.1'], W)
# CC2 is a row-1 pin boxed in by B4/B6: leave on the mouth side and go round
VC2B = (113.0, 116.35)
T('HOST_CC2', 'B', ['J1.B5', (109.7, 104.45), (115.75, 104.45), (115.75, 115.9), (115.3, 116.35), VC2B], W, r=0.8); V('HOST_CC2', VC2B)
T('HOST_CC2', 'F', ['U3.14', VC2B], W); T('HOST_CC2', 'F', [VC2B, 'R7.1'], W)

# ---------------------------------------------------------------- target port centre
T('GND', 'F', ['J2.A1', 'J2.B12'], 0.25); T('GND', 'F', ['J2.A12', 'J2.B1'], 0.25)
T('TGT_VBUS', 'F', ['J2.A4', 'J2.B9'], 0.25); T('TGT_VBUS', 'F', ['J2.A9', 'J2.B4'], 0.25)
V6T, V7T, VC1T, VS1T = (110.25, YJ - 5.4), (110.75, YJ - 4.75), (109.7, YJ - 4.75), (111.3, 135.6)
T('TGT_D_P', 'F', ['J2.A6', V6T], 0.2); V('TGT_D_P', V6T)
T('TGT_D_N', 'F', ['J2.A7', V7T], 0.2); V('TGT_D_N', V7T)
T('TGT_CC1', 'F', ['J2.A5', (109.75, YJ - 4.15), VC1T], W); V('TGT_CC1', VC1T)
# SBU1 runs up the middle of the protection zone and between the TVS arrays, so
# its via clears the VBUS neck
T('TGT_SBU1', 'F', ['J2.A8', (111.25, YJ - 6.0), (110.5, YJ - 6.75), (110.5, 137.2), (111.3, 136.4), VS1T], W, r=0.3); V('TGT_SBU1', VS1T)
# B.Cu under the pads belongs to VBUS here, so D- joins on In1 and D+ on In2
T('TGT_D_N', 'I1', [V7T, (110.75, YJ - 3.7), 'J2.B7'], 0.2, r=0.6)
T('TGT_D_P', 'I2', [V6T, (110.25, YJ - 4.0), 'J2.B6'], 0.2, r=0.6)

# ---------------------------------------------------------------- hub: target and MCU ports
# port 1 (east pins) -> ESD -> vias -> B.Cu trunk east, then south down the centre
T('TGT_D_P', 'F', ['U5.10', (106.4, 119.75), 'D11.6'], 0.2, r=0.5); T('TGT_D_P', 'F', ['D11.6', 'D11.5'], 0.2)
T('TGT_D_N', 'F', ['U5.9', (105.9, 120.25), 'D11.7'], 0.2, r=0.3); T('TGT_D_N', 'F', ['D11.7', 'D11.4'], 0.2)
VTN, VTP = (105.9, 126.3), (106.45, 126.85)
T('TGT_D_N', 'F', ['D11.4', VTN], 0.2); T('TGT_D_P', 'F', ['D11.5', (106.4, 126.4), VTP], 0.2, r=0.3)
V('TGT_D_N', VTN); V('TGT_D_P', VTP)
R.pair('TGT_D_N', 'TGT_D_P', 'B', [(107.2, 126.6), (110.5, 126.6), (110.5, YJ - 6.2)],
       w=0.2, gap=0.15, r=0.9, ends=((VTN, VTP), (V7T, V6T)))
# port 2 (south pins) -> vias -> B.Cu east under the corridor to the MCU
VMN, VMP = (103.6, 125.2), (104.2, 125.75)
T('MCU_D_N', 'F', ['U5.7', (104.05, 121.6), (103.75, 122.15), (103.75, 124.8), VMN], 0.2, r=0.4)
T('MCU_D_P', 'F', ['U5.8', (104.55, 121.55), (104.1, 122.15), (104.1, 125.3), VMP], 0.2, r=0.4)
V('MCU_D_N', VMN); V('MCU_D_P', VMP)
VN, VP = (112.3, 127.9), (113.25, 128.55)
R.pair('MCU_D_N', 'MCU_D_P', 'B', [(104.9, 125.475), (111.3, 125.475), (111.3, 128.075), (111.7, 128.075)],
       w=0.2, gap=0.15, r=0.8, ends=((VMN, VMP), (VN, VP)))
V('MCU_D_N', VN); V('MCU_D_P', VP)
T('MCU_D_N', 'F', ['U1.26', (112.9, 127.4), VN], 0.2, r=0.4)
T('MCU_D_P', 'F', ['U1.27', (113.35, 127.8), VP], 0.2, r=0.3)
T('MCU_D_P', 'F', [VP, 'R1.2'], W)

# ---------------------------------------------------------------- target CC, VBUS, SBU
VC1P, VC1M = (102.25, 130.85), (107.45, 130.0)
T('TGT_CC1', 'B', [VC1T, (109.45, YJ - 5.15), (109.45, 130.4), (109.15, 130.0), (102.7, 130.0), VC1P], W, r=0.6)
V('TGT_CC1', VC1P); V('TGT_CC1', VC1M)
T('TGT_CC1', 'F', ['U2.10', VC1P], W); T('TGT_CC1', 'F', [VC1M, 'R30.1'], W)
T('TMUX_CC1', 'F', ['R30.2', (108.6, 130.75), (108.85, 131.0), 'U4.13'], W, r=0.1)
VC2P, VC2M = (105.95, 131.05), (106.95, 131.75)
T('TGT_CC2', 'B', ['J2.B5', (111.3, YJ - 0.2), (105.4, YJ - 0.2), (104.9, YJ - 0.7), (104.9, 133.75),
                   (105.45, 133.2), (105.95, 132.7), VC2P], W, r=0.4)
T('TGT_CC2', 'B', [(105.45, 133.2), (106.5, 133.2), (106.95, 132.75), VC2M], W, r=0.3)
V('TGT_CC2', VC2P); V('TGT_CC2', VC2M)
T('TGT_CC2', 'F', ['U2.1', VC2P], W); T('TGT_CC2', 'F', [VC2M, 'R31.1'], W)
T('TMUX_CC2', 'F', ['R31.2', 'U4.14'], W)
# VBUS: mux -> F.Cu trunk west (over the SBU2 run) -> three vias -> B.Cu trunk
# -> neck between the D- via and the lane-2 fan -> bar joining B9 and B4
YV = 137.0
T('TGT_VBUS', 'F', ['U7.8', 'U7.1'], 0.4, r=0)
T('TGT_VBUS', 'F', [(121.95, 136.1), (121.35, 137.0)], 0.5, r=0)
T('TGT_VBUS', 'F', [(121.35, 137.0), (115.2, 137.0)], 1.0, r=0)
for x in (115.2, 116.0, 116.8): V('TGT_VBUS', (x, YV), d=0.6, drill=0.3)
YB = YJ - 3.25
# the neck stays east of the centre channel through the protection zone (the
# bleed resistors' ground vias need the room) and steps in above the lane-2 fan
T('TGT_VBUS', 'B', [(116.8, YV), (112.6, YV), (112.6, YJ - 6.0)], 1.0, r=0.5)
T('TGT_VBUS', 'B', [(112.6, YV + 0.6), (112.6, YJ - 6.0)], 1.2, r=0)
T('TGT_VBUS', 'B', [(112.6, YJ - 6.0), (111.55, YJ - 4.95), (111.55, YB)], 0.8, r=0.3)
T('TGT_VBUS', 'B', [(109.45, YB), (111.55, YB)], 0.8, r=0)
T('TGT_VBUS', 'B', ['J2.B9', (109.35, YB)], 0.35, r=0); T('TGT_VBUS', 'B', ['J2.B4', (111.65, YB)], 0.35, r=0)
# VBUS sense for the PD controller
VVS = (106.1, 136.95)
T('TGT_VBUS', 'B', [(109.3, YB), (109.05, YB - 0.55), (109.05, 137.3), (108.7, 136.95), VVS], 0.2, r=0.4); V('TGT_VBUS', VVS)
T('TGT_VBUS', 'F', [VVS, (106.45, 136.5), (106.45, 130.8), (106.0, 130.35), 'U2.2'], W, r=0.3)
T('TGT_VBUS', 'F', ['R27.1', (106.45, 134.5)], W); T('TGT_VBUS', 'F', ['R42.1', (106.45, 135.6)], W)
# SBU1 goes up the right side on B.Cu; SBU2 is another boxed-in row-1 pin and
# has to cross CC2's mouth-side run, so it takes In2 to the right first
VS1, VS2 = (117.05, 121.4), (117.05, 118.4)
# Both SBU lines detour through a series FET (gate at 3.3 V) below the VBUS
# trunk before they go up to the level shifters.
VS1U, VS1P, VS2A, VS2P, VSG, VSGT = (118.2, 144.3), (119.0, 140.6), (115.9, YJ - 0.2), (120.2, 142.7), (121.5, 141.0), (121.5, 130.6)
T('TGT_SBU1', 'B', [VS1T, (111.3, 135.3), (111.8, 134.8), (117.7, 134.8), (118.2, 135.3), VS1U], W, r=0.4); V('TGT_SBU1', VS1U)
T('TGT_SBU1', 'F', [VS1U, (119.1, 143.8), 'Q5.3'], W, r=0)
T('SBU1_P', 'F', ['Q5.2', VS1P], W); V('SBU1_P', VS1P)
T('SBU1_P', 'B', [VS1P, (119.0, 134.4), (118.6, 134.0), (117.5, 134.0), (117.05, 133.5), VS1], W, r=0.3); V('SBU1_P', VS1)
T('TGT_SBU2', 'I2', ['J2.B8', (109.7, YJ - 0.2), VS2A], W, r=0.4); V('TGT_SBU2', VS2A)
T('TGT_SBU2', 'F', [VS2A, (119.5, 146.8), (121.4, 146.8), 'Q6.3'], W, r=0.5)
T('SBU2_P', 'F', ['Q6.2', VS2P], W); V('SBU2_P', VS2P)
T('SBU2_P', 'B', [VS2P, (120.2, 118.0), (119.8, 117.6), (117.5, 117.6), VS2], W, r=0.5); V('SBU2_P', VS2)
V('SBU_G', VSG); V('SBU_G', VSGT); T('SBU_G', 'B', [VSGT, VSG], W)
T('SBU_G', 'F', [VSG, (120.75, 141.3), 'Q5.1'], W, r=0); T('SBU_G', 'F', [VSG, (122.35, 141.85), 'Q6.1'], W, r=0.2)
T('SBU_G', 'F', ['R46.2', 'R47.1'], W); T('SBU_G', 'F', [(123.3, 130.6), VSGT], W)

# ---------------------------------------------------------------- power
VA, VB2, V5A, VH5 = (114.9, 111.0), (118.55, 113.4), (109.0, 118.65), (104.9, 117.3)
# host 5 V carries up to the mux's host limit (1.5 A typ, 2 A max) plus the dongle. The
# first 3.5 mm is pinned at 0.3 mm between the CC1 via and lane 1; everything after is wide.
T('+5V', 'B', ['J1.B9', (111.8, 107.3), (111.8, 110.75), (112.2, 111.15)], WP, r=0.3)
T('+5V', 'B', [(112.2, 111.15), (114.6, 111.15), VA], 0.6, r=0.1); V('+5V', VA, d=0.8, drill=0.4)
T('+5V', 'F', [VA, (114.9, 110.2), (115.6, 109.3), (118.6, 109.3), 'C13.1'], 0.6, r=0.3)
T('+5V', 'F', ['U6.3', (118.55, 109.3)], WP)
T('+5V', 'F', ['C13.1', (119.45, 110.25), (119.45, 111.95), (119.1, 112.35), 'U6.1'], 0.2, r=0.3)
T('+5V', 'F', ['U6.1', VB2], WP); V('+5V', VB2, d=0.6, drill=0.3)
T('+5V', 'B', [VB2, (117.2, 114.75), (116.3, 115.2), (116.3, 119.5), (115.9, 119.9), (110.2, 119.9)], WP, r=0.8)
T('+5V', 'B', [(110.2, 119.9), (109.35, 119.9), (109.0, 119.55), V5A], WP, r=0.3); V('+5V', V5A, d=0.6, drill=0.3)
T('+5V', 'B', [(110.2, 119.9), (109.8, 120.3), (105.3, 120.3), (104.9, 119.9), VH5], WP, r=0.3); V('+5V', VH5, d=0.6, drill=0.3)
# host 5 V to the power mux: F.Cu across to the strip, then B.Cu straight down it
V5M, V5T = (122.65, 132.4), (122.5, 114.6)
T('+5V', 'F', ['C13.1', (119.08, 108.9), (120.7, 108.9), (120.7, 114.2), (122.1, 114.2), V5T], 0.5, r=0.3); V('+5V', V5T, d=0.8, drill=0.4)
T('+5V', 'F', [VB2, (119.35, 114.2), (120.7, 114.2)], WP, r=0.3)
T('+5V', 'B', [V5T, (122.5, 127.3)], 0.8, r=0); T('+5V', 'B', [(122.5, 127.3), (122.5, 131.9), V5M], 0.5, r=0.2); V('+5V', V5M, d=0.8, drill=0.4)
T('+5V', 'F', [V5M, 'C15.1'], 0.5, r=0); T('+5V', 'F', ['C15.1', 'U7.7'], 0.4, r=0)
T('+5V', 'F', [V5M, 'R2.1'], 0.3); T('+5V', 'F', ['R2.1', 'R46.1'], 0.25); T('+5V', 'F', [V5T, 'R17.1'], 0.25)
T('+5V', 'F', ['U3.8', 'C6.1'], 0.25); T('+5V', 'F', ['C6.1', V5A], 0.25)
T('+5V', 'F', [VH5, 'C7.1'], 0.25); T('+5V', 'F', ['C7.1', (103.55, 117.5), 'U5.15'], 0.2, r=0.2)
T('HMUX_VDD', 'F', ['U3.7', 'C5.1'], 0.2)
T('HUB_3V3', 'F', ['U5.16', 'C8.1'], 0.25)
# +3V3: LDO -> lane down the right column -> B.Cu under the MCU
V3A, V3M, V3T, V3S = (115.15, 120.6), (116.0, 130.2), (112.9, 130.6), (114.9, 134.0)
T('+3V3', 'F', ['U6.5', 'C14.1'], WP)
T('+3V3', 'F', ['C14.1', (115.15, 113.3), V3A], 0.25, r=0.4); V('+3V3', V3A, d=0.6, drill=0.3)
T('+3V3', 'B', [V3A, (115.15, 129.3), V3M], 0.25, r=0.5); V('+3V3', V3M, d=0.6, drill=0.3)
T('+3V3', 'B', [(115.15, 129.3), (114.6, 130.0), (113.4, 130.0), V3T], 0.25, r=0.4); V('+3V3', V3T)
T('+3V3', 'B', [V3M, (116.0, 132.9), V3S], 0.25, r=0.5); V('+3V3', V3S)
T('+3V3', 'F', [V3M, 'C1.1'], 0.25); T('+3V3', 'F', ['U1.2', 'C1.1'], 0.2); T('+3V3', 'F', [V3M, 'C2.1'], 0.25)
T('+3V3', 'F', [V3T, 'C10.1'], 0.2); T('+3V3', 'F', ['U4.8', (112.6, 131.15), 'C10.1'], 0.2, r=0.2); T('+3V3', 'F', ['U4.7', (112.6, 131.65), 'C10.1'], 0.2, r=0.2)
T('+3V3', 'F', [V3S, 'SW1.3'], 0.25)
T('+3V3', 'F', ['SW1.3', 'SW1.4'], 0.2)
T('+3V3', 'F', [(113.7, 119.6), 'TP2.1'], 0.25)

# ---------------------------------------------------------------- the under-corridor field
# four slow nets cross the SuperSpeed corridor on B.Cu between two staggered via fields
FX_R = lambda k: (111.75 + 0.45 * k, 121.0 + 0.55 * k)
FX_L = lambda k: (109.25 - 0.45 * k, 121.0 + 0.55 * k)
for k, net in ((0, '+3V3'), (2, 'PD_INT'), (3, 'I2C_SCL'), (4, 'I2C_SDA')):
    w = 0.25 if net == '+3V3' else W
    V(net, FX_R(k)); V(net, FX_L(k)); T(net, 'B', [FX_R(k), FX_L(k)], w)
# right side: MCU north pins fan into the field, each wrapping outside the last
T('I2C_SDA', 'F', ['U1.21', (114.8, 124.4), FX_R(4)], W, r=0.4)
T('I2C_SCL', 'F', ['U1.20', (115.2, 123.95), (114.25, 123.0), (113.9, 122.65), FX_R(3)], W, r=0.4)
T('PD_INT', 'F', ['U1.19', (115.6, 123.55), (114.5, 122.45), (114.15, 122.1), FX_R(2)], W, r=0.4)
T('+3V3', 'F', [(115.15, 119.6), (112.45, 119.6), (111.75, 120.3), FX_R(0)], 0.25, r=0.5)
# left side: straight down, then west into the PD controller's north pins
for k, net, pin, y in ((4, 'I2C_SDA', 'U2.7', 127.45), (3, 'I2C_SCL', 'U2.6', 127.8), (2, 'PD_INT', 'U2.5', 128.15), (0, '+3V3', 'U2.4', 128.5)):
    x, _ = FX_L(k); px, _ = P(pin)
    T(net, 'F', [FX_L(k), (x, y), (px, y), pin], 0.25 if net == '+3V3' else W, r=0.5)
T('+3V3', 'F', ['U2.4', 'U2.3'], 0.2); T('+3V3', 'F', ['U2.3', 'C3.1'], 0.2)

# ---------------------------------------------------------------- hub crystal
T('HUB_XI', 'F', ['Y1.1', (101.3, 115.3), (101.3, 118.85), (101.7, 119.25), 'U5.2'], W, r=0.4)
T('HUB_XO', 'F', ['Y1.3', (102.6, 112.55), (102.6, 115.4), (102.5, 115.9), (102.5, 118.3), 'U5.1'], W, r=0.3)

# ---------------------------------------------------------------- SBU level-shifter cells
T('SBU_PU', 'F', ['U1.17', (116.4, 123.0), (115.87, 122.4), 'R8.1'], W, r=0.4)
T('SBU_PU', 'F', ['R8.1', 'R10.1'], W); T('SBU_PU', 'F', ['R10.1', 'R12.1'], W)
T('SBU1_P', 'F', ['R8.2', VS1], 0.2); T('SBU1_P', 'F', [VS1, 'Q1.2'], 0.2); T('SBU1_P', 'F', ['R9.1', 'R8.2'], 0.2)
T('SBU2_P', 'F', ['R10.2', VS2], 0.2); T('SBU2_P', 'F', [VS2, 'Q2.2'], 0.2); T('SBU2_P', 'F', ['R11.1', 'Q2.2'], 0.2)
T('SBU_VG', 'F', ['R12.2', 'R13.1'], W); T('SBU_VG', 'F', ['R13.1', 'C12.1'], W)
T('SBU_VG', 'F', ['C12.1', (120.25, 116.25), (120.25, 117.75), 'Q2.1'], W, r=0.3)
T('SBU_VG', 'F', ['Q2.1', 'Q1.1'], W)
# MCU side: each net has two MCU pins, joined on B.Cu. PA4..PB0 leave the east
# side on F.Cu to the charger strip, so nothing wraps it on F.Cu.
VQ1A, VQ1B = (117.65, 129.95), (117.65, 123.85)
T('SBU1_MCU', 'F', ['U1.15', (117.2, 124.3), (118.45, 124.3), 'Q1.3'], W, r=0.4); T('SBU1_MCU', 'F', [VQ1B, (117.65, 124.3)], W)
T('SBU1_MCU', 'F', ['U1.7', (117.2, 129.5), VQ1A], W, r=0.2); V('SBU1_MCU', VQ1A); V('SBU1_MCU', VQ1B)
T('SBU1_MCU', 'B', [VQ1A, VQ1B], W)
VD2, VD2B, VD2C = (118.9, 121.25), (118.75, 125.75), (118.9, 128.6)
T('SBU2_MCU', 'F', ['Q2.3', VD2], 0.2); V('SBU2_MCU', VD2); V('SBU2_MCU', VD2B); V('SBU2_MCU', VD2C)
T('SBU2_MCU', 'B', [VD2, (118.9, 125.3), VD2B, (118.9, 126.1), VD2C], W, r=0.1)
T('SBU2_MCU', 'F', [VD2B, 'U1.14'], W); T('SBU2_MCU', 'F', [VD2C, (118.5, 128.2), 'U1.8'], W, r=0.2)

# ---------------------------------------------------------------- debug pads, boot, LEDs
T('SWCLK', 'F', ['U1.24', (113.2, 126.6), (113.2, 125.6), (112.75, 124.95), 'TP9.1'], W, r=0.4)
T('SWDIO', 'F', ['U1.25', (113.25, 127.0), (112.85, 126.8), 'TP10.1'], W, r=0.3)
T('BOOT_SW', 'F', ['R1.1', (114.3, 130.03), (114.55, 130.3), (114.55, 132.0), (114.9, 132.5), 'SW1.1'], W, r=0.4)
T('BOOT_SW', 'F', ['SW1.1', (116.25, 133.6), (118.55, 133.6), 'SW1.2'], W, r=0.5)
T('LED_STAT', 'F', ['U1.6', (116.8, 130.4), 'R18.1'], W, r=0.3); T('LEDS_A', 'F', ['R18.2', 'D2.1'], W)
T('TGT_CC1', 'F', ['U2.10', 'U2.11'], W); T('TGT_CC2', 'F', ['U2.1', 'U2.14'], W)

# ---------------------------------------------------------------- debug header and Tag-Connect
# SWCLK, SWDIO and 3V3 cross the corridor on B.Cu just below the field, run
# down the left edge and end on the header holes; the Tag-Connect pads hang
# off the header on F.Cu.
VK, VD, VDB = (112.4, 123.65), (112.3, 125.58), (101.75, 136.6)
T('SWCLK', 'F', ['TP9.1', VK], W); V('SWCLK', VK); T('SWDIO', 'F', ['TP10.1', VD], W); V('SWDIO', VD)
T('SWCLK', 'B', [VK, (112.1, 123.95), (101.7, 123.95), (101.35, 124.3), (101.35, 138.25), (101.7, 138.6), 'TP11.1'], W, r=0.3)
T('SWDIO', 'B', [VD, (111.5, 124.6), (102.1, 124.6), (101.75, 124.95), VDB], W, r=0.3); V('SWDIO', VDB)
T('SWDIO', 'F', [VDB, (101.75, 141.14), 'TP12.1'], W, r=0.2)
T('+3V3', 'B', [FX_L(0), (101.3, 121.0), (100.95, 121.35), (100.95, 143.3), (101.33, 143.68), 'TP13.1'], 0.2, r=0.3)
T('SWCLK', 'F', ['TP11.1', (104.4, 138.6), (104.4, 150.9), 'J4.4'], W, r=0.3)
T('SWDIO', 'F', ['TP12.1', (104.05, 141.14), (104.05, 149.63), 'J4.2'], W, r=0.3)
T('+3V3', 'F', ['TP13.1', (101.3, 143.68), (101.3, 149.2), 'J4.1'], W, r=0.3)
T('GND', 'F', ['J4.5', (100.9, 152.17)], W); V('GND', (100.9, 152.17))

# ---------------------------------------------------------------- charger strip
# power mux straps
T('MUX_SS', 'F', ['U7.11', 'C18.1'], W)
T('MUX_ILM', 'F', ['U7.10', (120.7, 135.05), (120.5, 135.2), (117.73, 135.2), (117.73, 135.75)], W, r=0.1)
T('MUX_ILM', 'F', ['R48.1', 'R35.2'], W); T('MUX_ILM_D', 'F', ['R48.2', 'R49.1'], W); T('GND', 'F', ['R35.1', 'C18.2'], W)
T('MUX_ILM', 'F', [(120.3, 135.2), 'R16.1'], W)
T('MUX_ILM_A', 'F', ['R16.2', 'R19.1'], W); T('MUX_ILM_B', 'F', ['R19.2', 'R20.1'], W)
T('MUX_ILM_C', 'F', ['R20.2', (121.83, 132.15), (121.5, 132.47), 'R34.2'], W, r=0.1)
T('MUX_ST', 'F', ['R34.1', (121.05, 134.0), 'U7.9'], W, r=0.1)
# charger port: B.Cu bar joins B4 and B9 under the pads, like the target port
T('GND', 'F', ['J3.A1', 'J3.B12'], 0.25); T('GND', 'F', ['J3.A12', 'J3.B1'], 0.25)
T('VBUS_IN', 'F', ['J3.A4', 'J3.B9'], 0.25); T('VBUS_IN', 'F', ['J3.A9', 'J3.B4'], 0.25)
T('VBUS_IN', 'B', [(124.5, 107.65), (126.5, 107.65)], 1.0, r=0)
T('VBUS_IN', 'B', ['J3.B4', (124.3, 107.65)], 0.45, r=0); T('VBUS_IN', 'B', ['J3.B9', (126.7, 107.65)], 0.45, r=0)
# charger VBUS: B.Cu out from the bar, then F.Cu down the right edge to the mux
XV = 129.8
T('VBUS_IN', 'B', [(126.5, 107.65), (127.3, 108.45), (128.4, 109.5), (XV, 110.0), (XV, 111.8)], 1.2, r=0.5)
for y in (110.0, 110.9, 111.8): V('VBUS_IN', (XV, y), d=0.6, drill=0.3)
T('VBUS_IN', 'F', [(XV, 110.0), (XV, 138.4), (122.65, 138.4)], 1.2, r=0.8)
T('VBUS_IN', 'F', ['C16.1', (122.65, 137.4), 'U7.2'], 0.4, r=0)
T('VBUS_IN', 'F', [(122.65, 139.0), (122.65, 136.9)], 0.8, r=0)
T('VBUS_IN', 'F', ['R22.1', (128.03, 138.0)], 0.25)
# heat: the mux has no thermal pad, its heat leaves through the IN2 and OUT
# pins. Tie the charger input to a block of B.Cu (about 70 mm2) through vias
# next to the pin and along the feed.
for xy in ((122.65, 138.0), (124.6, 138.9), (125.8, 138.9), (127.0, 138.9)): V('VBUS_IN', xy, d=0.6, drill=0.3)
T('VBUS_IN', 'B', [(122.65, 138.0), (124.2, 140.0)], 0.8, r=0)
T('VBUS_IN', 'B', [(124.2, 140.0), (129.6, 140.0), (129.6, 142.6), (124.2, 142.6), (124.2, 145.2), (129.6, 145.2)], 2.8, r=0)
# PD sink under the charger port. CC2 comes straight off the A row; CC1 is a
# boxed-in row-1 pin and goes round the mouth side on B.Cu like the host's CC2
T('VBUS_IN', 'F', ['J3.A9', (124.25, 108.4), (124.0, 108.8), 'U8.8'], 0.2, r=0.2)
T('VBUS_IN', 'F', [(XV, 113.0), (122.75, 113.0), (122.75, 111.53), 'U8.1'], 0.25, r=0.3)
T('VBUS_IN', 'F', ['C19.1', (122.75, 112.3)], 0.25)
T('CHG_CC2', 'F', ['J3.A5', (126.25, 108.45), (125.3, 109.05), (124.8, 109.25), 'U8.6'], W, r=0.2)
VCC = (124.75, 108.75)
T('CHG_CC1', 'B', ['J3.B5', (124.7, 104.45), (120.4, 104.45), (120.4, 108.75), VCC], W, r=0.6); V('CHG_CC1', VCC)
T('CHG_CC1', 'F', [VCC, (124.4, 109.15), 'U8.7'], W, r=0.1)
T('CHG_CFG', 'F', ['U8.9', (123.6, 108.9), 'R26.1'], W, r=0.2)
VSCB, VSDB, VSCT, VSDT = (123.45, 112.4), (124.15, 112.4), (123.2, 128.3), (123.75, 127.75)
T('CHG_SCL', 'F', ['U8.2', (123.6, 112.0), VSCB], W, r=0.1); T('CHG_SDA', 'F', ['U8.3', (124.0, 112.0), VSDB], W, r=0.1)
for n, v in (('CHG_SCL', VSCB), ('CHG_SDA', VSDB), ('CHG_SCL', VSCT), ('CHG_SDA', VSDT)): V(n, v)
T('CHG_SCL', 'B', [VSCB, (123.3, 112.9), (123.3, 127.8), VSCT], W, r=0.2)
T('CHG_SDA', 'B', [VSDB, (123.85, 112.9), (123.85, 127.3), VSDT], W, r=0.2)
T('CHG_SCL', 'F', ['U1.9', (122.7, 127.8), VSCT], W, r=0.3)
T('CHG_SDA', 'F', ['U1.10', (123.4, 127.4), VSDT], W, r=0.2)
# enables
T('HOST_EN_N', 'F', ['U1.11', (124.16, 127.0), 'R2.2'], W, r=0.2)
T('HOST_EN_N', 'F', ['R2.2', 'R21.1'], W); T('HOST_EN_N', 'F', ['R21.1', (124.16, 135.05), 'U7.5'], W, r=0.2)
T('CHG_HV', 'F', ['U1.13', (127.75, 126.2), (127.75, 132.53), 'R25.1'], W, r=0.6); T('CHG_HV', 'F', ['R25.1', 'Q3.1'], W)
T('CHG_EN', 'F', ['U1.12', (126.7, 126.6), 'Q4.1'], W, r=0.3); T('CHG_EN', 'F', ['Q4.1', 'R45.1'], W)
T('CHG_LIM', 'F', ['Q4.3', 'R43.2'], W); T('CHG_OV', 'F', ['R43.1', (125.75, 135.55)], W)
T('CHG_OV', 'F', ['U7.4', 'Q3.3'], W)
T('CHG_SNS', 'F', ['U7.3', (126.3, 136.05), 'R39.1'], W, r=0)
T('CHG_SNS', 'F', [(125.1, 136.05), (125.1, 137.05)], W); T('CHG_SNS', 'F', ['R24.1', 'R23.2'], W)
T('CHG_OV_A', 'F', ['R23.1', 'R22.2'], W)
T('CHG_DM', 'F', ['U8.5', 'R41.1'], W); T('CHG_DP', 'F', ['U8.4', (124.4, 111.9), (124.65, 112.3), 'R40.1'], W, r=0.1)
T('LEDP_A', 'F', ['R17.2', 'D1.1'], W)

# ---------------------------------------------------------------- ground
# exposed pads get their own vias (placed between the B.Cu traces passing underneath)
for xy in ((110.2, 116.1), (111.15, 131.9), (116.1, 126.3), (116.1, 127.7), (103.6, 119.2), (124.0, 110.6)):
    V('GND', xy)
T('GND', 'F', ['C10.2', (113.75, 130.72)], 0.2); V('GND', (113.75, 130.72))
# ground for the two bleed resistors that point into the centre channel, and for the limit resistor's pocket
T('GND', 'F', ['R63.2', (109.875, 143.75)], W); V('GND', (109.875, 143.75))
T('GND', 'F', ['R65.2', (111.4, 143.75)], W); V('GND', (111.4, 143.75))
T('GND', 'F', ['R49.2', (114.75, 135.32)], W); V('GND', (114.75, 135.32))
# return-path vias beside the lane pairs' layer changes
for xy in ((114.75, 109.0), (106.25, 110.0), (105.4, 143.6), (115.6, 143.6)): V('GND', xy)
# the skew bumps close off each TVS array's ground-pin column, so it gets its own via
for xy in ((107.9, 111.8), (113.1, 111.8), (107.9, 136.1), (113.1, 136.1)): V('GND', xy)

# ---------------------------------------------------------------- probe pads
T('+5V', 'F', [V5T, 'TP22.1'], 0.25); T('TGT_VBUS', 'F', [VVS, 'TP20.1'], W)
T('TGT_CC1', 'F', [VC1P, 'TP23.1'], W); T('TGT_CC2', 'F', ['U2.14', (104.85, 132.3), 'TP24.1'], W, r=0.2)
T('TGT_CC1', 'F', ['C22.1', VC1P], W); T('TGT_CC2', 'F', [VC2P, 'C23.1'], W)
V('CHG_SCL', (123.3, 121.0)); T('CHG_SCL', 'F', [(123.3, 121.0), 'TP25.1'], W)
V('CHG_SDA', (123.85, 119.5)); T('CHG_SDA', 'F', [(123.85, 119.5), 'TP26.1'], W)
T('CHG_OV', 'F', ['TP27.1', (125.75, 134.75)], W)
T('CHG_HV', 'F', ['TP28.1', (124.6, 126.2)], W)
T('TGT_SBU1', 'F', [VS1U, 'TP30.1'], W); T('TGT_SBU2', 'F', ['TP31.1', (120.5, 146.8)], W)
T('SBU_G', 'F', ['TP32.1', (122.35, 142.2)], W); T('SBU_G', 'F', ['Q6.1', 'C21.1'], W)
# the mux's strap pins sit in pockets the pour cannot reach
T('GND', 'F', ['U7.12', (120.75, 136.2), (119.6, 136.2), 'C18.2'], W, r=0.1); V('GND', (119.6, 136.0))
T('GND', 'F', ['C15.2', (122.0, 132.75)], W); V('GND', (122.0, 132.75))
T('GND', 'F', ['R24.2', (123.6, 136.55)], W); V('GND', (123.6, 136.55))
T('GND', 'F', ['U7.6', (123.5, 134.4), (123.5, 133.6)], W, r=0.1); V('GND', (123.5, 133.6))

R.save('dongle-pro-v2-pd.kicad_pcb')
import runpy
runpy.run_path(os.path.join(HERE, 'project.py'))['write_pro']()
print(R.count, 'track pieces')
sys.stdout.flush(); os._exit(0)
