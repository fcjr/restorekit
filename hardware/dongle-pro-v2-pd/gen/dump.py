#!/usr/bin/env python3
"""Dump pad positions/nets to /tmp/v2pd-pads.txt for route planning (KiCad python)."""
import os, sys
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import pcbnew
b = pcbnew.LoadBoard('dongle-pro-v2-pd.kicad_pcb')
out = []
for fp in sorted(b.GetFootprints(), key=lambda f: f.GetReference()):
    for p in fp.Pads():
        pos = p.GetPosition(); sz = p.GetSize()
        out.append(f"{fp.GetReference()}.{p.GetNumber()} {pos.x/1e6:.3f} {pos.y/1e6:.3f} {sz.x/1e6:.2f}x{sz.y/1e6:.2f} {'TH' if p.GetAttribute()==pcbnew.PAD_ATTRIB_PTH else 'NPTH' if p.GetAttribute()==pcbnew.PAD_ATTRIB_NPTH else 'SMD'} {p.GetNetname() or '-'}")
open('/tmp/v2pd-pads.txt', 'w').write("\n".join(out) + "\n")
print(len(out), "pads"); sys.stdout.flush(); os._exit(0)
