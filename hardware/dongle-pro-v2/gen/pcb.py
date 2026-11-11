#!/usr/bin/env python3
"""Build dongle-pro-v2.kicad_pcb (placement + outline) from gen/board.py.
Run with KiCad's bundled python (needs pcbnew):
  KPY=/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/Current/bin/python3
  $KPY -u gen/pcb.py

21 x 48 mm, 4-layer (JLC04161H-7628), single-sided assembly. Host receptacle on
the top edge, target on the bottom edge; the SuperSpeed pairs run straight down
the middle between the two CH9441P muxes. In1 and In2 are both solid GND so the
F.Cu and B.Cu pairs each have an adjacent reference plane.
"""
import re, sys, types, os
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import pcbnew

stub = types.ModuleType('gen'); stub.build_wired = lambda *a, **k: None
sys.modules['gen'] = stub
ns = {'__file__': os.path.abspath('gen/board.py')}
exec(compile(open('gen/board.py').read(), 'board.py', 'exec'), ns)
comps = [c for c in ns['comps'] if not c['ref'].startswith('#')]

libsrc = open('lib/dongle-lite.kicad_sym').read()
def fp_for(sym):
    i = libsrc.find(f'(symbol "{sym}"')
    m = re.search(r'"Footprint"\s*\n?\s*"[^:"]*:([^"]*)"', libsrc[i:i+8000])
    return m.group(1)

X0, Y0, X1, Y1 = 100.0, 100.0, 121.0, 148.0
CX = (X0 + X1) / 2
RAD = 1.6   # board corner radius
P = {}
exec(open('gen/place.py').read(), {'P': P, 'CX': CX, 'X0': X0, 'Y0': Y0, 'X1': X1, 'Y1': Y1})

board = pcbnew.CreateEmptyBoard()
board.SetFileName(os.path.abspath('dongle-pro-v2.kicad_pcb'))
board.SetCopperLayerCount(4)
board.SetLayerName(pcbnew.In1_Cu, 'GND1')
board.SetLayerName(pcbnew.In2_Cu, 'GND2')

netinfo = {}
for c in comps:
    for net in c['nets'].values():
        if net and net not in netinfo:
            ni = pcbnew.NETINFO_ITEM(board, net)
            board.Add(ni)
            netinfo[net] = ni

# no-connect pins: give their pads the schematic's "unconnected-(...)" nets so
# the board matches the schematic netlist name for name (parity check)
nc = {}
if os.path.exists('/tmp/v2.net'):
    for blk in re.split(r'\(net\s+\(code', open('/tmp/v2.net').read())[1:]:
        name = re.search(r'\(name "([^"]*)"\)', blk).group(1)
        if name.startswith('unconnected-'):
            for ref, pin in re.findall(r'\(node\s+\(ref "([^"]+)"\)\s+\(pin "([^"]+)"\)', blk):
                nc[(ref, pin)] = name
for name in sorted(set(nc.values())):
    ni = pcbnew.NETINFO_ITEM(board, name); board.Add(ni); netinfo[name] = ni

missing_place, missing_pad = [], []
for c in comps:
    ref = c['ref']
    if ref not in P:
        missing_place.append(ref); continue
    fp = pcbnew.FootprintLoad(os.path.abspath('lib/dongle-lite.pretty'), fp_for(c['sym']))
    fp.SetFPID(pcbnew.LIB_ID('dongle-lite', fp_for(c['sym'])))
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
        if net is None: net = nc.get((ref, pin))
        if net is None: continue
        if pin not in pads_by_num:
            missing_pad.append(f"{ref}.{pin}"); continue
        for pad in pads_by_num[pin]:
            pad.SetNet(netinfo[net])
    board.Add(fp)

def edge(shape, *pts):
    s = pcbnew.PCB_SHAPE(board); s.SetShape(shape)
    if shape == pcbnew.SHAPE_T_ARC:
        s.SetArcGeometry(*[pcbnew.VECTOR2I_MM(*p) for p in pts])
    else:
        s.SetStart(pcbnew.VECTOR2I_MM(*pts[0])); s.SetEnd(pcbnew.VECTOR2I_MM(*pts[1]))
    s.SetLayer(pcbnew.Edge_Cuts); s.SetWidth(pcbnew.FromMM(0.05)); board.Add(s)
R = RAD; K = R * (1 - 0.7071067811865476)
edge(pcbnew.SHAPE_T_SEGMENT, (X0+R, Y0), (X1-R, Y0)); edge(pcbnew.SHAPE_T_SEGMENT, (X1, Y0+R), (X1, Y1-R))
edge(pcbnew.SHAPE_T_SEGMENT, (X1-R, Y1), (X0+R, Y1)); edge(pcbnew.SHAPE_T_SEGMENT, (X0, Y1-R), (X0, Y0+R))
edge(pcbnew.SHAPE_T_ARC, (X1-R, Y0), (X1-K, Y0+K), (X1, Y0+R))
edge(pcbnew.SHAPE_T_ARC, (X1, Y1-R), (X1-K, Y1-K), (X1-R, Y1))
edge(pcbnew.SHAPE_T_ARC, (X0+R, Y1), (X0+K, Y1-K), (X0, Y1-R))
edge(pcbnew.SHAPE_T_ARC, (X0, Y0+R), (X0+K, Y0+K), (X0+R, Y0))

pcbnew.SaveBoard('dongle-pro-v2.kicad_pcb', board)
_pro_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py')
pro = {'__file__': _pro_path}
exec(compile(open(_pro_path).read(), 'project.py', 'exec'), pro)
pro['write_pro']()
print(f"placed {len(comps)-len(missing_place)}/{len(comps)}")
if missing_place: print("NO PLACEMENT:", missing_place)
if missing_pad: print("NO PAD:", missing_pad)
sys.stdout.flush(); os._exit(0)
