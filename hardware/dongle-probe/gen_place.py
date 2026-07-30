#!/usr/bin/env python3
"""Place dongle-probe footprints from measured pad+courtyard boxes. Run with KiCad's python."""
import pcbnew

BOARD = "layout/layout.kicad_pcb"
X0, Y0, X1, Y1 = 100.0, 100.0, 116.0, 138.4  # 16 x 38.4 mm strip
GAP = 0.3

ROT = {
    "J1": 180, "U2": 90, "C1": 90, "C2": 90, "C3": 90, "C9": 90,
    "R1": 0, "D3": 0, "R2": 0, "C13": 0, "C14": 0, "U1": 0,
    "R5": 90, "C11": 90, "C12": 90, "C5": 90, "C10": 90,
    "Y1": 0, "C6": 90, "C7": 90, "R4": 90, "L1": 0, "C4": 0, "C8": 90, "R3": 0,
    "SW1": 0, "R9": 90, "D1": 90, "R10": 90, "D2": 90,
    "R6": 0, "R7": 0, "R8": 0, "R11": 0, "J2": 90, "JP1": 0,
}

b = pcbnew.LoadBoard(BOARD)
fps = {}
for ref, rot in ROT.items():
    fp = b.FindFootprintByReference(ref)
    assert fp, ref
    fp.SetOrientationDegrees(rot)
    fps[ref] = fp

def box(ref):
    fp = fps[ref]
    bb = fp.GetCourtyard(pcbnew.F_CrtYd).BBox()
    if bb.GetWidth() == 0:
        bb = fp.GetBoundingBox(False)
    x0, y0, x1, y1 = bb.GetLeft(), bb.GetTop(), bb.GetRight(), bb.GetBottom()
    for pad in fp.Pads():
        pb = pad.GetBoundingBox()
        x0 = min(x0, pb.GetLeft()); y0 = min(y0, pb.GetTop())
        x1 = max(x1, pb.GetRight()); y1 = max(y1, pb.GetBottom())
    return x0/1e6, y0/1e6, x1/1e6, y1/1e6

def place_center(ref, cx, cy):
    x0, y0, x1, y1 = box(ref)
    fp = fps[ref]
    p = fp.GetPosition()
    dx = cx - (x0 + x1) / 2
    dy = cy - (y0 + y1) / 2
    fp.SetPosition(pcbnew.VECTOR2I(int(p.x + pcbnew.FromMM(dx)), int(p.y + pcbnew.FromMM(dy))))

def place_top(ref, cx, ytop):
    x0, y0, x1, y1 = box(ref)
    place_center(ref, cx, ytop + (y1 - y0) / 2)

def stack(cx, ytop, refs):
    y = ytop
    for ref in refs:
        x0, y0, x1, y1 = box(ref)
        h = y1 - y0
        place_center(ref, cx, y + h / 2)
        y += h + GAP

# USB-C: shell recessed ~1.4mm from top edge (same as dongle-lite)
place_top("J1", 108.0, 101.4)
j1b = box("J1")[3]

row_y = j1b + GAP           # R1 / D3 / R2 row under the connector
place_top("R1", 105.0, row_y)
place_top("D3", 108.0, row_y)
place_top("R2", 111.4, row_y)
cap_y = row_y + 2.1 + GAP   # C13 / C14 above the QFN
place_top("C13", 105.3, cap_y)
place_top("C14", 110.7, cap_y)
u1_top = cap_y + 1.1 + GAP
place_top("U1", 108.0, u1_top)
u1 = box("U1")

stack(102.2, j1b + GAP, ["U2", "C1", "C2", "C3", "C9"])       # left rail
stack(114.6, j1b + GAP, ["R5", "C11", "C12", "C5", "C10"])    # right rail

band = u1[3] + GAP          # below the QFN
place_top("Y1", 104.7, band)
place_top("C6", 107.2, band)
place_top("C7", 108.6, band)
place_top("R4", 110.1, band)
place_top("L1", 112.3, band)
place_top("C8", 114.8, band)
band2 = band + 2.9 + GAP
place_top("C4", 110.6, band2)
place_top("R3", 113.6, band2)

sw_top = band2 + 1.1 + GAP
place_top("SW1", 108.0, sw_top)
sw = box("SW1")
stack(101.9, sw[1] + 0.6, ["R9", "D1"])                       # LED columns
stack(114.2, sw[1] + 0.6, ["R10", "D2"])

r_y = sw[3] + GAP           # SWD series resistor row
for ref, cx in [("R6", 103.7), ("R7", 105.8), ("R8", 107.9), ("R11", 110.0)]:
    place_top(ref, cx, r_y)

j2_y = r_y + 1.05 + GAP
place_top("J2", 108.0, j2_y)
place_top("JP1", 113.3, j2_y + 0.4)

# refs to fab layer to keep silk clean
for fp in b.GetFootprints():
    ref = fp.Reference()
    ref.SetLayer(pcbnew.F_Fab)
    ref.SetVisible(True)

# outline
for d in list(b.GetDrawings()):
    if d.GetLayerName() == "Edge.Cuts":
        b.Remove(d)
def seg(x1, y1, x2, y2):
    s = pcbnew.PCB_SHAPE(b)
    s.SetShape(pcbnew.SHAPE_T_SEGMENT)
    s.SetStart(pcbnew.VECTOR2I(pcbnew.FromMM(x1), pcbnew.FromMM(y1)))
    s.SetEnd(pcbnew.VECTOR2I(pcbnew.FromMM(x2), pcbnew.FromMM(y2)))
    s.SetLayer(pcbnew.Edge_Cuts)
    s.SetWidth(pcbnew.FromMM(0.1))
    b.Add(s)
seg(X0, Y0, X1, Y0); seg(X1, Y0, X1, Y1); seg(X1, Y1, X0, Y1); seg(X0, Y1, X0, Y0)

pcbnew.SaveBoard(BOARD, b)
print("placed; J2 box", [round(v, 2) for v in box("J2")], "board bottom", Y1)
