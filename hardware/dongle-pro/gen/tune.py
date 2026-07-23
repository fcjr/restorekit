#!/usr/bin/env python3
"""Intra-pair skew tuning: measure each SuperSpeed pair's P/N length mismatch
and null it with 45-degree serpentine bumps on the shorter member, inserted
into a known straight run of its route. Amplitudes are computed from the live
measurement, so the step is self-correcting across pipeline reruns.
Run with KiCad's bundled python after gen/ses.py, before gen/zones.py.
"""
import os
os.chdir(os.path.join(os.path.dirname(__file__), '..'))
import pcbnew

PCB = 'dongle-pro.kicad_pcb'
board = pcbnew.LoadBoard(PCB)
MM = pcbnew.FromMM
V = pcbnew.VECTOR2I_MM
SQRT2_M1 = 0.41421356  # extra length per unit amplitude per diagonal pair

# (pair base, layer of the tuning run, x of the straight vertical,
#  bump direction, [y_lo, y_hi] window, bump count, flat, gap)
TUNE = [
    ('HUB_DRX', 'Sig2', None, +1, (142.3, 150.2), 3, 0.4, 0.4),
    ('HUB_DTX', 'F.Cu', None, -1, (140.0, 144.4), 2, 0.4, 0.4),
    ('TGT_TX1', 'F.Cu', None, -1, (167.1, 171.9), 3, 0.4, 0.3),
    ('TGT_TX2', 'F.Cu', None, -1, (158.8, 165.6), 4, 0.35, 0.3),
    ('TGT_RX2', 'F.Cu', None, -1, (162.4, 166.4), 2, 0.4, 0.4),
]
MIN_SKEW = 0.25  # mm; below this the pair is left alone

def net_len(name):
    return sum(t.GetLength() / 1e6 for t in board.GetTracks()
               if t.Type() == pcbnew.PCB_TRACE_T and t.GetNetname() == name)

def find_vertical(net, layer_name, window):
    """The straight vertical segment of `net` on `layer_name` spanning `window`."""
    for t in board.GetTracks():
        if t.Type() != pcbnew.PCB_TRACE_T or t.GetNetname() != net:
            continue
        if board.GetLayerName(t.GetLayer()) != layer_name:
            continue
        s, e = t.GetStart(), t.GetEnd()
        x1, y1, x2, y2 = s.x / 1e6, s.y / 1e6, e.x / 1e6, e.y / 1e6
        if abs(x1 - x2) > 0.01:
            continue
        if min(y1, y2) <= window[0] and max(y1, y2) >= window[1]:
            return t
    return None

def add_seg(net_obj, layer, a, b, w):
    t = pcbnew.PCB_TRACK(board)
    t.SetStart(V(*a)); t.SetEnd(V(*b))
    t.SetLayer(layer); t.SetWidth(MM(w))
    t.SetNet(net_obj); board.Add(t)

nets = {n.GetNetname(): n for n in board.GetNetInfo().NetsByName().values()}
report = []
to_remove = []
to_add = []  # (net_obj, layer, pts, width)

for base, layer_name, _x, sign, window, n, flat, gap in TUNE:
    lp, ln = net_len(base + '_P'), net_len(base + '_N')
    short = base + ('_P' if lp < ln else '_N')
    skew = abs(lp - ln)
    if skew < MIN_SKEW:
        report.append(f'{base}: skew {skew:.3f}mm, left alone')
        continue
    seg = find_vertical(short, layer_name, window)
    if seg is None:
        report.append(f'{base}: !! no tuning segment found for {short}')
        continue
    amp = skew / (n * 2 * SQRT2_M1)
    s, e = seg.GetStart(), seg.GetEnd()
    x = s.x / 1e6
    ya, yb = sorted((s.y / 1e6, e.y / 1e6))
    w = seg.GetWidth() / 1e6
    layer = seg.GetLayer()
    span = n * (2 * amp + flat) + (n - 1) * gap
    y0 = (window[0] + window[1] - span) / 2
    if y0 < ya or y0 + span > yb:
        report.append(f'{base}: !! window does not fit inside the segment')
        continue
    pts = [(x, ya), (x, y0)]
    y = y0
    for _ in range(n):
        pts += [(x + sign * amp, y + amp),
                (x + sign * amp, y + amp + flat),
                (x, y + 2 * amp + flat)]
        y += 2 * amp + flat + gap
    pts += [(x, yb)]
    to_remove.append(seg)
    to_add.append((nets[short], layer, pts, w))
    report.append(f'{base}: {short} +{skew:.3f}mm via {n} bumps, amp {amp:.3f}mm')

for seg in to_remove:
    board.Remove(seg)
for net_obj, layer, pts, w in to_add:
    for a, b in zip(pts, pts[1:]):
        if a != b:
            add_seg(net_obj, layer, a, b, w)

pcbnew.SaveBoard(PCB, board)
_pro_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py')
pro = {'__file__': _pro_path}
exec(compile(open(_pro_path).read(), 'project.py', 'exec'), pro)
pro['write_pro']()
for line in report:
    print(line)
