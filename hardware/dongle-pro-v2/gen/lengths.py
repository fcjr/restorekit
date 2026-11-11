#!/usr/bin/env python3
"""Report routed length per differential pair and the P/N skew (KiCad python)."""
import os, sys, collections
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import pcbnew
b = pcbnew.LoadBoard('dongle-pro-v2.kicad_pcb')
L = collections.defaultdict(float); nv = collections.Counter()
for t in b.GetTracks():
    n = t.GetNetname()
    if t.Type() == pcbnew.PCB_VIA_T: nv[n] += 1
    else: L[n] += t.GetLength() / 1e6
for base in sorted({n[:-2] for n in L if n.endswith(('_P', '_N'))}):
    p, n = L.get(base + '_P', 0), L.get(base + '_N', 0)
    print(f'{base:10s} P {p:6.2f}  N {n:6.2f}  skew {abs(p - n):5.2f} mm  vias {nv[base + "_P"]}/{nv[base + "_N"]}')
sys.stdout.flush(); os._exit(0)
