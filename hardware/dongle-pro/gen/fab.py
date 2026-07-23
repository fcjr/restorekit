#!/usr/bin/env python3
"""Generate the JLCPCB fab package (gerbers, drill, BOM, CPL, STEP) into
mfg-jlcpcb/. Run with KiCad's bundled python after the full board pipeline.
"""
import os, csv, subprocess, zipfile, collections
os.chdir(os.path.join(os.path.dirname(__file__), '..'))
import pcbnew

PCB = 'dongle-pro.kicad_pcb'
OUT = 'mfg-jlcpcb'
GERBER_DIR = os.path.join(OUT, 'gerbers')
os.makedirs(GERBER_DIR, exist_ok=True)

LAYERS = 'F.Cu,GND1.Cu,Sig2.Cu,B.Cu,F.Paste,B.Paste,F.Silkscreen,B.Silkscreen,F.Mask,B.Mask,Edge.Cuts'
subprocess.run(['kicad-cli', 'pcb', 'export', 'gerbers',
                '--layers', LAYERS, '--subtract-soldermask',
                '-o', GERBER_DIR + '/', PCB], check=True)
subprocess.run(['kicad-cli', 'pcb', 'export', 'drill',
                '--format', 'excellon', '--excellon-separate-th',
                '-o', GERBER_DIR + '/', PCB], check=True)

zf = zipfile.ZipFile(os.path.join(OUT, 'dongle-pro-gerbers-jlcpcb.zip'), 'w',
                     zipfile.ZIP_DEFLATED)
for f in sorted(os.listdir(GERBER_DIR)):
    zf.write(os.path.join(GERBER_DIR, f), f)
zf.close()

subprocess.run(['kicad-cli', 'pcb', 'export', 'step', '--subst-models',
                '-o', os.path.join(OUT, 'dongle-pro.step'), PCB], check=False)

# ---- BOM / CPL ----
LCSC = {
    '22pF': 'C1804', '1uF': 'C52923', '100nF': 'C1525', '4.7uF': 'C368809',
    '10uF': 'C15850', 'KT-0603R': 'C2286', 'USBLC6-2SC6': 'C7519',
    'TPD4E05U06DQAR_C138714': 'C138714', 'TYPE-C24PQT': 'C2681555',
    '3.3uH': 'C42411119', '33': 'C25105', '1k': 'C11702', '910k': 'C25800',
    '200k': 'C25764', '20k': 'C25765', '100k': 'C25741', '10k': 'C25744',
    '4.7k': 'C25900', '47k': 'C25819', 'TS-1187A-B-A-B': 'C318884',
    'RP2354A': 'C41378174', 'AP22653W6-7': 'C2158037',
    'FUSB302BMPX': 'C132291', 'HD3SS3220RNHR': 'C165155',
    'GL3510-OSY52': 'C7501408', 'RT9013-33GB': 'C47773',
    'TLV70212DBVR': 'C81462', 'HD3SS3212IRKSR': 'C544517',
    '74AVC1T45GW,125': 'C282330', 'X322512MSB4SI': 'C9002',
    '25MHz': 'C9006',
}
MPN = {'25MHz': 'X322525MOB4SI', 'TPD4E05U06DQAR_C138714': 'TPD4E05U06DQAR'}
SKIP_PREFIX = ('TP', 'J3')

def short_fp(name):
    n = name.split(':')[-1]
    for pre in ('R0402', 'R0603', 'C0402', 'C0805'):
        if n.startswith(pre):
            return pre
    return n

board = pcbnew.LoadBoard(PCB)
groups = collections.defaultdict(list)
cpl_rows = []
for f in board.GetFootprints():
    ref = f.GetReference()
    if any(ref.startswith(p) and ref[len(p):].isdigit() or ref == p for p in SKIP_PREFIX):
        continue
    val = f.GetValue()
    fp = short_fp(str(f.GetFPID().GetLibItemName()))
    groups[(val, fp)].append(ref)
    pos = f.GetPosition()
    cpl_rows.append((ref, val, fp, pos.x / 1e6, -pos.y / 1e6,
                     f.GetOrientationDegrees(),
                     'bottom' if f.IsFlipped() else 'top'))

def refkey(r):
    i = 0
    while i < len(r) and not r[i].isdigit():
        i += 1
    return (r[:i], int(r[i:]) if i < len(r) else 0)

with open(os.path.join(OUT, 'bom.csv'), 'w', newline='') as fh:
    w = csv.writer(fh)
    w.writerow(['Designator', 'Comment', 'Footprint', 'Qty', 'LCSC Part', 'MPN'])
    for (val, fp), refs in sorted(groups.items(), key=lambda kv: refkey(sorted(kv[1], key=refkey)[0])):
        refs = sorted(refs, key=refkey)
        w.writerow([','.join(refs), val, fp, len(refs),
                    LCSC.get(val, ''), MPN.get(val, '')])

with open(os.path.join(OUT, 'cpl.csv'), 'w', newline='') as fh:
    w = csv.writer(fh)
    w.writerow(['Designator', 'Val', 'Package', 'Mid X', 'Mid Y', 'Rotation', 'Layer'])
    for row in sorted(cpl_rows, key=lambda r: refkey(r[0])):
        w.writerow([row[0], row[1], row[2], f'{row[3]:.6f}', f'{row[4]:.6f}',
                    f'{row[5]:.6f}', row[6]])

print('fab package written to', OUT)
