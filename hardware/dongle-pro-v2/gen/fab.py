#!/usr/bin/env python3
"""JLCPCB fab package into mfg-jlcpcb/: gerbers + drill (zipped), BOM, CPL, STEP.
Run with KiCad's python after gen/build.sh. LCSC codes come from the symbol
library, so the BOM cannot drift from the schematic."""
import os, re, csv, sys, types, subprocess, zipfile, collections
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import pcbnew

PCB, NAME, OUT = 'dongle-pro-v2.kicad_pcb', 'dongle-pro-v2', 'mfg-jlcpcb'
GERBER_DIR = os.path.join(OUT, 'gerbers')
os.makedirs(GERBER_DIR, exist_ok=True)
for f in os.listdir(GERBER_DIR): os.remove(os.path.join(GERBER_DIR, f))

stub = types.ModuleType('gen'); stub.build_wired = lambda *a, **k: None
sys.modules['gen'] = stub
ns = {'__file__': os.path.abspath('gen/board.py')}
exec(compile(open('gen/board.py').read(), 'board.py', 'exec'), ns)
comps = {c['ref']: c for c in ns['comps'] if not c['ref'].startswith('#')}
lib = open('lib/dongle-lite.kicad_sym').read()
def sym_prop(sym, prop):
    i = lib.find(f'(symbol "{sym}"')
    m = re.search(r'"%s"\s*\n?\s*"([^"]*)"' % re.escape(prop), lib[i:i + 8000])
    return m.group(1) if m else ''

LAYERS = 'F.Cu,In1.Cu,In2.Cu,B.Cu,F.Paste,B.Paste,F.Silkscreen,B.Silkscreen,F.Mask,B.Mask,Edge.Cuts'
subprocess.run(['kicad-cli', 'pcb', 'export', 'gerbers', '--layers', LAYERS, '--subtract-soldermask',
                '-o', GERBER_DIR + '/', PCB], check=True, stdout=subprocess.DEVNULL)
subprocess.run(['kicad-cli', 'pcb', 'export', 'drill', '--format', 'excellon', '--excellon-separate-th',
                '-o', GERBER_DIR + '/', PCB], check=True, stdout=subprocess.DEVNULL)
with zipfile.ZipFile(os.path.join(OUT, f'{NAME}-gerbers-jlcpcb.zip'), 'w', zipfile.ZIP_DEFLATED) as zf:
    for f in sorted(os.listdir(GERBER_DIR)): zf.write(os.path.join(GERBER_DIR, f), f)
subprocess.run(['kicad-cli', 'pcb', 'export', 'step', '--subst-models', '-o', os.path.join(OUT, f'{NAME}.step'), PCB],
               check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

def refkey(r):
    i = next((k for k, ch in enumerate(r) if ch.isdigit()), len(r))
    return (r[:i], int(r[i:] or 0))

board = pcbnew.LoadBoard(PCB)
groups = collections.defaultdict(list); cpl = []; missing = []
for f in board.GetFootprints():
    ref = f.GetReference(); c = comps[ref]
    if not c.get('in_bom', True): continue
    lcsc = sym_prop(c['sym'], 'LCSC Part'); mpn = sym_prop(c['sym'], 'MPN') or c['sym']
    if not lcsc: missing.append(ref)
    fp = str(f.GetFPID().GetLibItemName())
    groups[(c.get('value') or mpn, fp, lcsc, mpn)].append(ref)
    pos = f.GetPosition()
    cpl.append((ref, c.get('value') or mpn, fp, pos.x / 1e6, -pos.y / 1e6, f.GetOrientationDegrees(), 'top'))

with open(os.path.join(OUT, 'bom.csv'), 'w', newline='') as fh:
    w = csv.writer(fh); w.writerow(['Designator', 'Comment', 'Footprint', 'Qty', 'LCSC Part', 'MPN'])
    for (val, fp, lcsc, mpn), refs in sorted(groups.items(), key=lambda kv: refkey(sorted(kv[1], key=refkey)[0])):
        refs = sorted(refs, key=refkey); w.writerow([','.join(refs), val, fp, len(refs), lcsc, mpn])
with open(os.path.join(OUT, 'cpl.csv'), 'w', newline='') as fh:
    w = csv.writer(fh); w.writerow(['Designator', 'Val', 'Package', 'Mid X', 'Mid Y', 'Rotation', 'Layer'])
    for row in sorted(cpl, key=lambda r: refkey(r[0])):
        w.writerow([row[0], row[1], row[2], f'{row[3]:.4f}', f'{row[4]:.4f}', f'{row[5]:.1f}', row[6]])
print(f'{len(groups)} BOM rows, {len(cpl)} placements; no LCSC code: {missing or "none"}')
sys.stdout.flush(); os._exit(0)
