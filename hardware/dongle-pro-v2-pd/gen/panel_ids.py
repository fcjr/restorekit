#!/usr/bin/env python3
"""Give every part on the panel its own designator (C1 -> C1_1, C1_2, C1_3) in
KiKit's JLCPCB BOM and placement files, so the placement file has no repeats.
usage: gen/panel_ids.py panel/mfg-jlcpcb"""
import csv, sys, os, collections
d = sys.argv[1]
rows = list(csv.reader(open(os.path.join(d, 'pos.csv'))))
seen = collections.Counter(); copies = collections.defaultdict(list)
for r in rows[1:]:
    seen[r[0]] += 1
    new = f'{r[0]}_{seen[r[0]]}'; copies[r[0]].append(new); r[0] = new
csv.writer(open(os.path.join(d, 'pos.csv'), 'w', newline='')).writerows(rows)
bom = list(csv.reader(open(os.path.join(d, 'bom.csv'))))
for r in bom[1:]:
    r[1] = ','.join(n for ref in r[1].split(',') for n in copies[ref.strip()])
csv.writer(open(os.path.join(d, 'bom.csv'), 'w', newline='')).writerows(bom)
n = set(seen.values())
print(f'{len(rows) - 1} placements, {len(seen)} parts x {sorted(n)} copies, {len(bom) - 1} BOM lines')
