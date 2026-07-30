#!/usr/bin/env python3
"""Post-route finishing: USB_DM jumper, stub cleanup, GND zones + vias, fill. KiCad python."""
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
nets = b.GetNetsByName()
gnd = nets["GND"]
dm = nets["USB_DM"]

def mm(v):
    return pcbnew.FromMM(v)

def track(net, layer, x1, y1, x2, y2, w=0.15):
    t = pcbnew.PCB_TRACK(b)
    t.SetStart(pcbnew.VECTOR2I(mm(x1), mm(y1)))
    t.SetEnd(pcbnew.VECTOR2I(mm(x2), mm(y2)))
    t.SetWidth(mm(w))
    t.SetLayer(layer)
    t.SetNet(net)
    b.Add(t)

# join A7 to B7 through the clear corridor above the pin row
track(dm, pcbnew.F_Cu, 107.75, 109.005, 107.75, 107.95)
track(dm, pcbnew.F_Cu, 107.75, 107.95, 108.75, 107.95)
track(dm, pcbnew.F_Cu, 108.75, 107.95, 108.75, 109.005)

# drop dangling stubs (endpoint touching nothing else)
def clean_stubs():
    removed = 0
    items = [t for t in b.GetTracks() if t.GetClass() == "PCB_TRACK"]
    pads = [(p.GetPosition(), p) for fp in b.GetFootprints() for p in fp.Pads()]
    ends = {}
    for t in items:
        for pt in (t.GetStart(), t.GetEnd()):
            ends.setdefault((pt.x, pt.y), []).append(t)
    vias = [t for t in b.GetTracks() if t.GetClass() == "PCB_VIA"]
    for t in items:
        for pt in (t.GetStart(), t.GetEnd()):
            others = [o for o in ends.get((pt.x, pt.y), []) if o is not t]
            on_via = any(v.GetPosition() == pt for v in vias)
            on_pad = any(p.HitTest(pt) for _, p in pads)
            if not others and not on_via and not on_pad:
                b.Remove(t)
                removed += 1
                break
    return removed

total = 0
for _ in range(5):
    n = clean_stubs()
    total += n
    if n == 0:
        break
print("stubs removed:", total)

# rules: JLC-compatible minimums
bds = b.GetDesignSettings()
bds.m_CopperEdgeClearance = mm(0.2)
bds.m_MinClearance = mm(0.1)

# GND zones on both layers
outline = (100.0, 100.0, 116.0, 138.4)
for layer in (pcbnew.F_Cu, pcbnew.B_Cu):
    z = pcbnew.ZONE(b)
    pts = [(outline[0], outline[1]), (outline[2], outline[1]), (outline[2], outline[3]), (outline[0], outline[3])]
    chain = pcbnew.SHAPE_LINE_CHAIN()
    for x, y in pts:
        chain.Append(mm(x), mm(y))
    chain.SetClosed(True)
    z.Outline().AddOutline(chain)
    z.SetLayer(layer)
    z.SetNet(gnd)
    z.SetLocalClearance(mm(0.2))
    z.SetMinThickness(mm(0.15))
    z.SetPadConnection(pcbnew.ZONE_CONNECTION_THERMAL)
    z.SetIslandRemovalMode(pcbnew.ISLAND_REMOVAL_MODE_ALWAYS)
    b.Add(z)

def via(x, y):
    v = pcbnew.PCB_VIA(b)
    v.SetPosition(pcbnew.VECTOR2I(mm(x), mm(y)))
    v.SetDrill(mm(0.3))
    v.SetWidth(mm(0.6))
    v.SetNet(gnd)
    v.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
    b.Add(v)

u1 = b.FindFootprintByReference("U1")
ep = [p for p in u1.Pads() if p.GetName() == "61"][0].GetPosition()
ex, ey = ep.x / 1e6, ep.y / 1e6
for dx, dy in [(-0.8, -0.8), (0.8, -0.8), (-0.8, 0.8), (0.8, 0.8)]:
    via(ex + dx, ey + dy)

# stitching: corners, USB shell, button, header, crystal
for x, y in [(101.0, 101.0), (115.0, 101.0), (101.0, 137.4), (115.0, 137.4),
             (101.0, 121.0), (115.0, 121.0), (105.0, 106.5), (111.0, 106.5),
             (108.0, 121.4), (104.0, 128.0), (112.0, 128.0), (105.3, 136.6), (110.7, 136.6)]:
    via(x, y)

filler = pcbnew.ZONE_FILLER(b)
filler.Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("zones filled, saved")
