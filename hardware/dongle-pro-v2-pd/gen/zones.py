#!/usr/bin/env python3
"""GND pours on all four layers plus stitching vias, then fill. Run with KiCad's
python after gen/route.py. In1 is the unbroken reference under the F.Cu pairs;
In1 carries the target D- join, In2 three short jumpers (both D+ joins and
SBU2); both are GND otherwise.

Stitching is searched, not listed: every spot on a grid that clears all non-GND
copper on every layer (a through via touches all four) gets a via.
"""
import os, sys, math
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import pcbnew

PCB = 'dongle-pro-v2-pd.kicad_pcb'
board = pcbnew.LoadBoard(PCB)
gnd = board.GetNetInfo().NetsByName()['GND']
X0, Y0, X1, Y1 = 100.0, 100.0, 132.0, 155.0

tracks = [t for t in board.GetTracks() if t.GetNetname() != 'GND']
pads = [p for fp in board.GetFootprints() for p in fp.Pads()]
gvias = [(t.GetPosition().x / 1e6, t.GetPosition().y / 1e6) for t in board.GetTracks()
         if t.Type() == pcbnew.PCB_VIA_T and t.GetNetname() == 'GND']

def clear(x, y):
    pt = pcbnew.VECTOR2I_MM(x, y)
    for t in tracks:
        if t.HitTest(pt, pcbnew.FromMM(0.5)): return False
    for p in pads:
        # keep off every pad (no via-in-pad), further from other nets and holes
        d = 0.32 if p.GetNetname() == 'GND' and p.GetAttribute() == pcbnew.PAD_ATTRIB_SMD else 0.6
        if p.HitTest(pt, pcbnew.FromMM(d)): return False
    for gx, gy in gvias:
        if math.hypot(gx - x, gy - y) < 0.9: return False
    return True

def via(x, y):
    v = pcbnew.PCB_VIA(board)
    v.SetPosition(pcbnew.VECTOR2I_MM(x, y))
    v.SetWidth(pcbnew.F_Cu, pcbnew.FromMM(0.45)); v.SetDrill(pcbnew.FromMM(0.2))
    v.SetViaType(pcbnew.VIATYPE_THROUGH); v.SetNet(gnd); board.Add(v)
    gvias.append((x, y))

# 1) one via beside every GND SMD pad that can take one (ties the pad to the planes directly)
placed = 0
for fp in board.GetFootprints():
    for p in fp.Pads():
        if p.GetNetname() != 'GND' or p.GetAttribute() != pcbnew.PAD_ATTRIB_SMD: continue
        px, py = p.GetPosition().x / 1e6, p.GetPosition().y / 1e6
        if any(math.hypot(gx - px, gy - py) < 1.0 for gx, gy in gvias): continue
        best = None
        for rad in (0.6, 0.75, 0.9, 1.1):
            for k in range(16):
                a = k * math.pi / 8
                x, y = px + rad * math.cos(a), py + rad * math.sin(a)
                if not (X0 + 0.7 < x < X1 - 0.7 and Y0 + 0.7 < y < Y1 - 0.7): continue
                if clear(x, y): best = (x, y); break
            if best: break
        if best:
            via(round(best[0], 2), round(best[1], 2)); placed += 1
            t = pcbnew.PCB_TRACK(board)
            t.SetStart(pcbnew.VECTOR2I_MM(px, py)); t.SetEnd(pcbnew.VECTOR2I_MM(round(best[0], 2), round(best[1], 2)))
            t.SetLayer(pcbnew.F_Cu); t.SetWidth(pcbnew.FromMM(0.25)); t.SetNet(gnd); board.Add(t)

# 2) general stitching on a grid
grid = 0
y = Y0 + 1.0
while y < Y1 - 0.9:
    x = X0 + 1.0
    while x < X1 - 0.9:
        if all(math.hypot(gx - x, gy - y) >= 1.6 for gx, gy in gvias) and clear(x, y):
            via(x, y); grid += 1
        x += 0.8
    y += 0.8

def add_zone(layer):
    z = pcbnew.ZONE(board)
    pts = [(X0, Y0), (X1, Y0), (X1, Y1), (X0, Y1)]
    z.AddPolygon(pcbnew.VECTOR_VECTOR2I([pcbnew.VECTOR2I_MM(x, y) for x, y in pts]))
    z.SetLayer(layer); z.SetNet(gnd); z.SetAssignedPriority(0)
    z.SetLocalClearance(pcbnew.FromMM(0.25)); z.SetMinThickness(pcbnew.FromMM(0.2))
    z.SetPadConnection(pcbnew.ZONE_CONNECTION_FULL)
    z.SetIslandRemovalMode(pcbnew.ISLAND_REMOVAL_MODE_ALWAYS)
    z.SetIsFilled(False); board.Add(z)
for layer in (pcbnew.F_Cu, pcbnew.In1_Cu, pcbnew.In2_Cu, pcbnew.B_Cu):
    add_zone(layer)
filler = pcbnew.ZONE_FILLER(board)
filler.Fill(board.Zones())

# 3) outer-layer pour islands that hold a GND pad but no via are floating: find
# a via spot inside each one and refill. Anything left is reported.
thpads = [(p.GetPosition().x / 1e6, p.GetPosition().y / 1e6) for p in pads
          if p.GetNetname() == 'GND' and p.GetAttribute() == pcbnew.PAD_ATTRIB_PTH]
fixed, stuck = 0, []
for z in board.Zones():
    if z.GetLayer() not in (pcbnew.F_Cu, pcbnew.B_Cu): continue
    polys = z.GetFilledPolysList(z.GetLayer())
    for i in range(polys.OutlineCount()):
        isl = pcbnew.SHAPE_POLY_SET(); isl.AddOutline(polys.Outline(i))
        for h in range(polys.HoleCount(i)): isl.AddHole(polys.Hole(i, h))
        inside = lambda x, y: isl.Contains(pcbnew.VECTOR2I_MM(x, y))
        if any(inside(x, y) for x, y in gvias + thpads): continue
        bb = isl.BBox()
        x0, y0, x1, y1 = bb.GetLeft() / 1e6, bb.GetTop() / 1e6, bb.GetRight() / 1e6, bb.GetBottom() / 1e6
        spot = None
        yy = y0 + 0.3
        while yy < y1 and not spot:
            xx = x0 + 0.3
            while xx < x1:
                if inside(xx, yy) and X0 + 0.7 < xx < X1 - 0.7 and Y0 + 0.7 < yy < Y1 - 0.7 and clear(xx, yy) \
                   and all(inside(xx + dx, yy + dy) for dx, dy in ((0.3, 0), (-0.3, 0), (0, 0.3), (0, -0.3))):
                    spot = (round(xx, 2), round(yy, 2)); break
                xx += 0.1
            yy += 0.1
        if spot: via(*spot); fixed += 1
        else: stuck.append((board.GetLayerName(z.GetLayer()), round((x0 + x1) / 2, 2), round((y0 + y1) / 2, 2), round(x1 - x0, 2), round(y1 - y0, 2)))
if fixed: filler.Fill(board.Zones())
print(f'islands stitched {fixed}; still floating: {stuck}')
pcbnew.SaveBoard(PCB, board)
import runpy
runpy.run_path(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'project.py'))['write_pro']()
print(f'pad vias {placed}, grid vias {grid}, zones filled')
sys.stdout.flush(); os._exit(0)
