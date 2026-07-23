#!/usr/bin/env python3
"""Build dongle-pro.kicad_pcb from the board-usb3.py netlist.
Run with KiCad's bundled python (needs pcbnew):
  /Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/Current/bin/python3 gen/pcb-usb3.py

Single-sided 4-layer board (JLC04161H-7628), 26 x 98 mm. SS corridor runs
top (host J1) to bottom (target J2) through HD3SS3220 -> GL3510 -> HD3SS3212.
Placement only; SS routing + freerouting happen in later passes.
"""
import re, sys, types, os
os.chdir(os.path.join(os.path.dirname(__file__), '..'))
import pcbnew

# --- netlist from board-usb3.py (stub out gen so nothing is regenerated) ---
stub = types.ModuleType('gen'); stub.build_wired = lambda *a, **k: None
sys.modules['gen'] = stub
ns = {'__file__': os.path.abspath('gen/board.py')}
exec(compile(open('gen/board.py').read(), 'board.py', 'exec'), ns)
comps = [c for c in ns['comps'] if not c['ref'].startswith('#')]

# --- symbol -> footprint map from the symbol lib ---
libsrc = open('lib/dongle-lite.kicad_sym').read()
def fp_for(sym):
    i = libsrc.find(f'(symbol "{sym}"')
    m = re.search(r'"Footprint"\s*\n?\s*"[^:"]*:([^"]*)"', libsrc[i:i+8000])
    return m.group(1)

# --- placement table: ref -> (x mm, y mm, rot deg). Board x 96..122, y 99..197 ---
X0, Y0, X1, Y1 = 96.0, 99.0, 122.0, 197.0
P = {
 # host connector + front-end (top)
 "J1": (109.0, 104.1, 180),
 "D12": (111.9, 112.2, 0), "D13": (107.0, 112.2, 0),   # SS TVS, flow-through
 "D10": (103.5, 115.5, 0),                                  # host D+/- ESD
 "C31": (111.9, 116.3, 90), "C32": (110.7, 116.3, 90),      # M1_TX AC caps
 "C33": (105.7, 116.3, 90), "C34": (106.9, 116.3, 90),
 "U3": (109.0, 121.5, 0),                                   # HD3SS3220
 "R14": (118.5, 122.5, 90), "R15": (118.5, 125.0, 90),      # VBUS_DET / DIR
 "C28": (99.5, 119.5, 90), "C29": (99.5, 122.0, 90), "C30": (112.6, 124.3, 90),
 # hub (center-top)
 "U4": (109.0, 137.0, 0),                                   # GL3510 QFN-64
 "Y2": (98.6, 143.5, 90), "C15": (98.6, 139.5, 90), "C16": (98.6, 147.5, 90),
 "R16": (99.5, 130.5, 90),                                  # RTERM 20k
 "R17": (99.0, 133.5, 90), "R18": (101.5, 133.5, 90), "C37": (99.5, 136.5, 90),  # RESETJ
 "R19": (117.5, 130.5, 90), "R20": (119.5, 130.5, 90),      # VBUS sense
 "R21": (118.35, 132.7, 90), "R22": (119.5, 133.0, 90),      # FN_B / FN_C straps
 "L2": (118.5, 137.5, 0),                                   # hub 1.2V buck
 "C38": (118.5, 141.0, 0), "C39": (118.5, 143.5, 0),        # +5V hub
 "C40": (115.5, 148.2, 90),                                 # HUB_3V3 bulk
 "C41": (103.6, 132.5, 0), "C42": (103.6, 130.0, 0), "C43": (115.5, 145.5, 90), "C44": (97.1, 148.6, 90),
 "C45": (118.5, 146.5, 90), "C46": (118.5, 150.5, 90),      # HUB_1V2 bulk
 "C47": (106.0, 145.5, 90), "C48": (108.0, 145.5, 90), "C49": (110.0, 145.5, 90), "C50": (112.0, 145.5, 90),
 "C51": (115.5, 132.5, 90), "C52": (115.5, 130.0, 90),      # AVDD12
 # target mux + AC caps
 "C35": (103.45, 147.4, 270), "C36": (102.8, 145.2, 270),      # DS2 TX AC caps
 "U7": (106.0, 154.5, 270),                                   # HD3SS3212
 "R23": (108.6, 150.0, 0), "R24": (108.6, 152.0, 0),      # SEL / OEn
 "C53": (101.5, 158.1, 90), "C54": (101.5, 160.6, 90), "C55": (105.9, 150.2, 0),
 "D14": (106.6, 184.0, 0), "D15": (110.4, 184.0, 0),      # target SS TVS
 # MCU core (right-center)
 "U1": (116.2, 165.0, 0),                                   # RP2354A
 "L1": (120.0, 158.6, 0),
 "C6": (114.9, 160.1, 0), "C7": (120.1, 160.7, 0), "C9": (120.9, 165.6, 90),
 "R12": (120.9, 162.2, 90),
 "Y1": (115.75, 171.2, 0), "C1": (113.05, 171.2, 90), "C2": (119.0, 171.2, 90),
 "R1": (120.9, 179.4, 90), "R13": (114.8, 158.5, 90),
 "SW1": (100.5, 164.0, 0),                                  # BOOTSEL at right edge
 "C4": (113.4, 174.6, 0), "C5": (115.4, 174.6, 0), "C8": (117.4, 174.6, 0),
 "C10": (119.3, 177.2, 0), "C11": (113.4, 176.9, 0), "C12": (115.4, 176.9, 0), "C27": (97.5, 176.8, 90),
 # PD + SBU (left-bottom, near J2 CC/SBU pins)
 "U2": (100.5, 176.0, 0),                                   # FUSB302B
 "R2": (97.8, 169.3, 90), "R3": (97.8, 172.0, 90), "R4": (97.8, 174.7, 90),
 "C13": (103.5, 172.5, 90), "C14": (103.5, 175.5, 90),
 "U8": (98.4, 183.2, 0), "U9": (101.9, 183.2, 0),          # SBU shifters
 "C20": (97.5, 179.5, 0), "C21": (103.9, 178.4, 90),
 # power (right-bottom, near J2 VBUS)
 "U5": (118.5, 182.5, 90), "R25": (113.8, 182.0, 90), "C56": (115.6, 182.0, 90),
 "U6": (114.0, 186.5, 90),
 "U10": (118.5, 189.0, 90),
 "R8": (116.0, 190.0, 90), "R9": (116.3, 186.0, 90),
 "C22": (120.9, 174.0, 90), "C23": (117.3, 178.8, 90), "C24": (97.5, 187.0, 90),
 "C25": (120.9, 171.5, 90), "C26": (116.0, 192.5, 0),
 "D11": (103.6, 187.0, 0),                                  # target D+/- ESD
 "D1": (98.0, 189.5, 0), "R10": (99.0, 186.5, 0),
 "D2": (101.75, 189.5, 0), "R11": (101.3, 186.5, 0),
 # debug
 "J3": (97.8, 154.5, 90),                                   # TC2030 (no-fit)
 "TP1": (97.5, 101.0, 0), "TP2": (100.0, 101.0, 0), "TP3": (102.5, 101.0, 0),
 "TP4": (97.5, 194.5, 0), "TP5": (100.0, 194.5, 0), "TP6": (102.5, 194.5, 0),
 "TP7": (97.5, 192.0, 0), "TP8": (99.2, 180.4, 0),
 "TP9": (120.5, 104.0, 0), "TP10": (120.5, 107.0, 0), "TP11": (120.5, 110.0, 0),
 # target connector (bottom)
 "J2": (109.0, 191.9, 0),
}

board = pcbnew.CreateEmptyBoard()
board.SetFileName(os.path.abspath('dongle-pro.kicad_pcb'))
board.SetCopperLayerCount(4)
board.SetLayerName(pcbnew.In1_Cu, 'GND1')
board.SetLayerName(pcbnew.In2_Cu, 'Sig2')

# nets
netinfo = {}
for c in comps:
    for net in c['nets'].values():
        if net and net not in netinfo:
            ni = pcbnew.NETINFO_ITEM(board, net)
            board.Add(ni)
            netinfo[net] = ni

missing_place, missing_pad = [], []
for c in comps:
    ref = c['ref']
    if ref not in P:
        missing_place.append(ref); continue
    fp = pcbnew.FootprintLoad(os.path.abspath('lib/dongle-lite.pretty'), fp_for(c['sym']))
    fp.SetReference(ref)
    fp.SetValue(c.get('value') or c['sym'])
    x, y, rot = P[ref]
    fp.SetPosition(pcbnew.VECTOR2I_MM(x, y))
    fp.SetOrientationDegrees(rot)
    if not c.get('in_bom', True):
        fp.SetAttributes(fp.GetAttributes() | pcbnew.FP_EXCLUDE_FROM_BOM)
    pads_by_num = {}
    for pad in fp.Pads():
        pads_by_num.setdefault(pad.GetNumber(), []).append(pad)
    for pin, net in c['nets'].items():
        if net is None: continue
        if pin not in pads_by_num:
            missing_pad.append(f"{ref}.{pin}"); continue
        for pad in pads_by_num[pin]:
            pad.SetNet(netinfo[net])
    board.Add(fp)

# board outline
for (ax, ay), (bx, by) in [((X0,Y0),(X1,Y0)), ((X1,Y0),(X1,Y1)),
                           ((X1,Y1),(X0,Y1)), ((X0,Y1),(X0,Y0))]:
    seg = pcbnew.PCB_SHAPE(board)
    seg.SetShape(pcbnew.SHAPE_T_SEGMENT)
    seg.SetStart(pcbnew.VECTOR2I_MM(ax, ay)); seg.SetEnd(pcbnew.VECTOR2I_MM(bx, by))
    seg.SetLayer(pcbnew.Edge_Cuts); seg.SetWidth(pcbnew.FromMM(0.05))
    board.Add(seg)

pcbnew.SaveBoard('dongle-pro.kicad_pcb', board)
# SaveBoard clobbers the sibling .kicad_pro with defaults — restore ours
_pro_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py')
pro = {'__file__': _pro_path}
exec(compile(open(_pro_path).read(), 'project.py', 'exec'), pro)
pro['write_pro']()
print(f"placed {len(comps)-len(missing_place)}/{len(comps)}")
if missing_place: print("NO PLACEMENT:", missing_place)
if missing_pad: print("NO PAD:", missing_pad)
