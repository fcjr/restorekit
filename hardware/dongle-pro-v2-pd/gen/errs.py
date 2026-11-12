#!/usr/bin/env python3
"""Compact list of DRC errors from /tmp/v2pd-drc.rpt. usage: gen/errs.py [type ...]"""
import re, sys
t = open('/tmp/v2pd-drc.rpt').read()
want = sys.argv[1:] or ['shorting_items', 'clearance', 'courtyards_overlap', 'hole_clearance', 'copper_edge_clearance',
                        'tracks_crossing', 'track_dangling', 'via_dangling', 'hole_to_hole', 'annular_width', 'unconnected_items']
for b in re.split(r'\n(?=\[)', t):
    k = b[1:b.find(']')]
    if k not in want: continue
    L = [l.strip() for l in b.strip().split('\n')]
    act = re.search(r'actual ([\d.]+)', L[0])
    items = [re.sub(r'^@\(([\d.]+) mm, ([\d.]+) mm\): ', r'(\1,\2) ', l).replace(' on F.Cu', ' F').replace(' on B.Cu', ' B')[:70] for l in L[2:4]]
    print(k[:10], act.group(1) if act else '', '|', ' || '.join(items))
