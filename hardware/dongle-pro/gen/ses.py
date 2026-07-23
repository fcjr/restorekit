#!/usr/bin/env python3
"""Surgical SES import: apply freerouting's session wires/vias, skipping any
segment/via that already exists on the board (protected wiring is re-echoed in
the SES; duplicate-aware import keeps the hand-routed copper authoritative).
Run with KiCad's bundled python after gen/route-usb3.py + freerouting.
"""
import os, re, sys
os.chdir(os.path.join(os.path.dirname(__file__), '..'))
import pcbnew

SES = sys.argv[1] if len(sys.argv) > 1 else 'dongle-pro.ses'
PCB = sys.argv[2] if len(sys.argv) > 2 else 'dongle-pro.kicad_pcb'

board = pcbnew.LoadBoard(PCB)
nets = {n.GetNetname(): n for n in board.GetNetInfo().NetsByName().values()}

def key(layer, a, b):
    p = (round(a[0]*100), round(a[1]*100)); q = (round(b[0]*100), round(b[1]*100))
    return (layer, min(p,q), max(p,q))
have_t, have_v = set(), set()
for t in board.GetTracks():
    if isinstance(t, pcbnew.PCB_VIA):
        pp = t.GetPosition(); have_v.add((round(pp.x/1e4), round(pp.y/1e4)))
    else:
        s0, e0 = t.GetStart(), t.GetEnd()
        have_t.add(key(t.GetLayer(), (s0.x/1e6, s0.y/1e6), (e0.x/1e6, e0.y/1e6)))
LAYER = {'F.Cu': pcbnew.F_Cu, 'GND1': pcbnew.In1_Cu, 'Sig2': pcbnew.In2_Cu,
         'B.Cu': pcbnew.B_Cu}

src = open(SES).read()
i = src.find('(network_out')
body = src[i:]

# tokenize net blocks
def blocks(s, tag):
    out = []
    for m in re.finditer(r'\(%s[\s("]' % tag, s):
        depth = 0; j = m.start()
        while True:
            c = s[j]
            if c == '(': depth += 1
            elif c == ')':
                depth -= 1
                if depth == 0: break
            j += 1
        out.append(s[m.start():j+1])
    return out

added_t = added_v = 0
skipped = set()
for nb in blocks(body, 'net'):
    name = nb.split(None, 2)[1].strip('"')
    if name not in nets:
        print('?? unknown net', name); continue
    net = nets[name]
    for wb in blocks(nb, 'wire'):
        m = re.search(r'\(path\s+(\S+)\s+(\d+)\s+([\s\d\.-]+)\)', wb)
        if not m: continue
        layer, width, coords = m.group(1), int(m.group(2)), m.group(3).split()
        pts = [(float(coords[k])/1e4, -float(coords[k+1])/1e4)
               for k in range(0, len(coords), 2)]
        for a, b in zip(pts, pts[1:]):
            if a == b: continue
            if key(LAYER[layer], a, b) in have_t:
                skipped.add(name); continue
            t = pcbnew.PCB_TRACK(board)
            t.SetStart(pcbnew.VECTOR2I_MM(*a)); t.SetEnd(pcbnew.VECTOR2I_MM(*b))
            t.SetLayer(LAYER[layer]); t.SetWidth(pcbnew.FromMM(width/1e4))
            t.SetNet(net); board.Add(t); added_t += 1
    for vm in re.finditer(r'\(via\s+"?Via\[[^"]*?_(\d+):(\d+)_um"?\s+(-?\d+)\s+(-?\d+)', nb):
        d, drill, x, y = (int(vm.group(1))/1000, int(vm.group(2))/1000,
                          float(vm.group(3))/1e4, -float(vm.group(4))/1e4)
        if (round(x*100), round(y*100)) in have_v:
            skipped.add(name); continue
        v = pcbnew.PCB_VIA(board)
        v.SetPosition(pcbnew.VECTOR2I_MM(x, y))
        v.SetWidth(pcbnew.FromMM(d)); v.SetDrill(pcbnew.FromMM(drill))
        v.SetViaType(pcbnew.VIATYPE_THROUGH)
        v.SetNet(net); board.Add(v); added_v += 1

# edge cleanup: freerouting has no board-edge clearance model
X0, Y0, X1, Y1 = 96.0, 99.0, 122.0, 197.0
M = 0.56
def bad(x, y): return x < X0+M or x > X1-M or y < Y0+M or y > Y1-M
drop = 0
for t in list(board.GetTracks()):
    s0, e0 = t.GetStart(), t.GetEnd()
    if bad(s0.x/1e6, s0.y/1e6) or bad(e0.x/1e6, e0.y/1e6):
        board.Remove(t); drop += 1
print('edge cleanup dropped', drop)

pcbnew.SaveBoard(PCB, board)
_pro_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py')
pro = {'__file__': _pro_path}
exec(compile(open(_pro_path).read(), 'project.py', 'exec'), pro)
pro['write_pro']()
print(f'added {added_t} tracks, {added_v} vias; {len(skipped)} nets had duplicates skipped')
