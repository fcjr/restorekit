#!/usr/bin/env python3
"""Layout audit (KiCad python): SuperSpeed lane lengths and skew end to end,
what sits close to the lanes on their own layer, anything routed on the
reference plane under them, ground vias near their layer changes, and how far
each IC's support parts are from the pins they serve."""
import os, sys, math, collections, re
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import pcbnew
b = pcbnew.LoadBoard('dongle-pro-v2-pd.kicad_pcb')
MM = pcbnew.FromMM
SS = re.compile(r'^(HOST|TGT|TGTC)_(TX|RX)[12]_[PN]$|^SS[AB]_[PN]$')
tracks = list(b.GetTracks())
is_via = lambda t: t.Type() == pcbnew.PCB_VIA_T
L = collections.defaultdict(float)
for t in tracks:
    if not is_via(t): L[t.GetNetname()] += t.GetLength() / 1e6
print('== lanes, end to end (mm)')
def tot(base, s): return L.get(f'{base}_{s}', 0)
for name, parts in (('host RX1', ['HOST_RX1']), ('host TX1', ['HOST_TX1']), ('host RX2', ['HOST_RX2']), ('host TX2', ['HOST_TX2']),
                    ('corridor A', ['SSA']), ('corridor B', ['SSB']),
                    ('target RX1', ['TGT_RX1', 'TGTC_RX1']), ('target TX1', ['TGT_TX1', 'TGTC_TX1']),
                    ('target RX2', ['TGT_RX2', 'TGTC_RX2']), ('target TX2', ['TGT_TX2', 'TGTC_TX2'])):
    p = sum(tot(x, 'P') for x in parts); n = sum(tot(x, 'N') for x in parts)
    print(f'  {name:11s} P {p:6.2f}  N {n:6.2f}  skew {abs(p-n):5.2f}')

def near(a, o, layer, d):
    try: return a.GetEffectiveShape(layer).Collide(o.GetEffectiveShape(layer), MM(d))
    except Exception: return False
pads = [p for fp in b.GetFootprints() for p in fp.Pads()]
def pair(n): return re.sub(r'_[PN]$', '', n)
print('== foreign copper within 0.30 mm of a lane, same layer (not ground, not its own pair)')
seen = set()
for t in tracks:
    n = t.GetNetname()
    if is_via(t) or not SS.match(n): continue
    lay = t.GetLayer()
    for o in tracks:
        on = o.GetNetname()
        if on in ('GND', n) or pair(on) == pair(n): continue
        if not is_via(o) and o.GetLayer() != lay: continue
        if near(t, o, lay, 0.30):
            d = next(x for x in (0.13, 0.16, 0.20, 0.25, 0.30) if near(t, o, lay, x))
            seen.add((n[:-2], b.GetLayerName(lay), ('via ' if is_via(o) else 'track ') + on, d))
    for p in pads:
        pn = p.GetNetname()
        if pn in ('GND', n) or pair(pn) == pair(n) or not p.IsOnLayer(lay): continue
        if near(t, p, lay, 0.30):
            d = next(x for x in (0.13, 0.16, 0.20, 0.25, 0.30) if near(t, p, lay, x))
            seen.add((n[:-2], b.GetLayerName(lay), f'pad {p.GetParentFootprint().GetReference()}.{p.GetNumber()} {pn or "nc"}', d))
for row in sorted(seen, key=lambda r: (r[3], r[0])): print(f'  <{row[3]:.2f}  {row[0]:9s} {row[1]:5s} {row[2]}')
print('== tracks on a reference plane that pass under a lane')
ref = {pcbnew.F_Cu: pcbnew.In1_Cu, pcbnew.B_Cu: pcbnew.In2_Cu}
hit = set()
for t in tracks:
    if is_via(t) or not SS.match(t.GetNetname()): continue
    for o in tracks:
        if is_via(o) or o.GetLayer() != ref.get(t.GetLayer()): continue
        if o.GetBoundingBox().Intersects(t.GetBoundingBox()) and t.GetEffectiveShape(t.GetLayer()).Collide(o.GetEffectiveShape(o.GetLayer()), 0):
            hit.add((t.GetNetname(), o.GetNetname()))
print('  ' + (', '.join(f'{a} over {c}' for a, c in sorted(hit)) or 'none'))
print('== lane vias: distance to the nearest ground via (mm)')
gv = [(t.GetPosition().x / 1e6, t.GetPosition().y / 1e6) for t in tracks if is_via(t) and t.GetNetname() == 'GND']
for t in tracks:
    if is_via(t) and SS.match(t.GetNetname()):
        x, y = t.GetPosition().x / 1e6, t.GetPosition().y / 1e6
        print(f'  {t.GetNetname():12s} ({x:.2f},{y:.2f})  {min(math.hypot(x-gx, y-gy) for gx, gy in gv):.2f}')
print('== power nets: narrowest copper and via count (IPC-2221 outer layer, 1 oz, 10 C rise: 0.3 mm ~1.0 A, 0.5 ~1.4, 0.8 ~2.0, 1.0 ~2.4, 1.2 ~2.7)')
for net in ('VBUS_IN', 'TGT_VBUS', '+5V', '+3V3'):
    w = collections.defaultdict(list); nv = 0
    for t in tracks:
        if t.GetNetname() != net: continue
        if is_via(t): nv += 1
        else: w[round(t.GetWidth() / 1e6, 2)].append(t.GetLength() / 1e6)
    print(f'  {net:9s} vias {nv:2d}  ' + '  '.join(f'{k:.2f} mm x {sum(v):5.1f}' for k, v in sorted(w.items())))
print('== support parts: pad-to-pin distance (mm)')
P = {(fp.GetReference(), p.GetNumber()): (p.GetPosition().x / 1e6, p.GetPosition().y / 1e6) for fp in b.GetFootprints() for p in fp.Pads()}
def dist(a, c): return math.hypot(P[a][0] - P[c][0], P[a][1] - P[c][1])
for what, a, c in (('MCU VDD 100nF', ('C1', '1'), ('U1', '2')), ('MCU VDD 1uF', ('C2', '1'), ('U1', '2')),
                   ('HUSB311 VDD', ('C3', '1'), ('U2', '3')), ('host mux VDD', ('C5', '1'), ('U3', '7')), ('host mux VBUS', ('C6', '1'), ('U3', '8')),
                   ('target mux VDD', ('C10', '1'), ('U4', '8')), ('hub V5', ('C7', '1'), ('U5', '15')), ('hub 3V3', ('C8', '1'), ('U5', '16')),
                   ('hub crystal XI', ('Y1', '1'), ('U5', '2')), ('hub crystal XO', ('Y1', '3'), ('U5', '1')),
                   ('LDO in', ('C13', '1'), ('U6', '1')), ('LDO out', ('C14', '1'), ('U6', '5')),
                   ('power mux IN1 1uF', ('C15', '1'), ('U7', '7')), ('power mux IN2 4.7uF', ('C16', '1'), ('U7', '2')),
                   ('power mux OUT 10uF', ('C17', '1'), ('U7', '1')), ('power mux OUT 10uF', ('C20', '1'), ('U7', '1')),
                   ('power mux SS', ('C18', '1'), ('U7', '11')), ('power mux ILM', ('R16', '1'), ('U7', '10')),
                   ('PD sink VHV 1uF', ('C19', '1'), ('U8', '1')), ('PD sink CFG1', ('R26', '1'), ('U8', '9')),
                   ('SBU gate ref', ('C12', '1'), ('Q2', '1')), ('host Rd CC1', ('R6', '1'), ('U3', '13')), ('host Rd CC2', ('R7', '1'), ('U3', '14'))):
    print(f'  {what:22s} {a[0]:4s} {dist(a, c):5.2f}')
sys.stdout.flush(); os._exit(0)
