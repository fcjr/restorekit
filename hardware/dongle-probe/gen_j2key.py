#!/usr/bin/env python3
"""Rework J2's footprint graphics for the Amphenol Minitek127 20021511-00006T4LF
keyed box header (same 2x3 1.27mm THT drill grid, bigger shrouded housing).
Key slot faces the pin-1/3/5 row (board bottom edge). KiCad python."""
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

CX, CY = 108.0, 135.81      # pad-field center
HL, HW = 6.35, 5.10         # housing
SLOT_W = 2.40               # polarization slot, south wall (odd-pin row side)

fp = None
for f in b.GetFootprints():
    if f.GetReference() == "J2":
        fp = f
        break

fp.SetValue("20021511-00006T4LF")
for k, v in (("Mpn", "20021511-00006T4LF"), ("MPN", "20021511-00006T4LF"),
             ("LCSC", "C5411311"), ("Manufacturer", "Amphenol ICC"),
             ("Description", "1.27mm 2x3 shrouded keyed box header"),
             ("Datasheet", "")):
    if k in fp.GetFieldsText():
        fp.SetField(k, v)

# strip existing outline graphics (keep text fields)
for g in list(fp.GraphicalItems()):
    if g.GetClass() in ("PCB_SHAPE", "MGRAPHIC") and g.GetLayerName() in (
            "F.SilkS", "F.Silkscreen", "F.Fab", "F.CrtYd", "F.Courtyard"):
        fp.Remove(g)

x0, x1 = CX - HL / 2, CX + HL / 2
y0, y1 = CY - HW / 2, CY + HW / 2
sx0, sx1 = CX - SLOT_W / 2, CX + SLOT_W / 2

def seg(layer, w, ax, ay, bx, by):
    s = pcbnew.PCB_SHAPE(fp)
    s.SetShape(pcbnew.SHAPE_T_SEGMENT)
    s.SetStart(P(ax, ay)); s.SetEnd(P(bx, by))
    s.SetWidth(mm(w))
    s.SetLayer(layer)
    fp.Add(s)

FAB = b.GetLayerID("F.Fab")
SILK = b.GetLayerID("F.SilkS")
CRT = b.GetLayerID("F.CrtYd")

for lay, w in ((FAB, 0.1), (SILK, 0.12)):
    seg(lay, w, x0, y0, x1, y0)                      # north wall
    seg(lay, w, x0, y1, sx0, y1)                     # south wall, west of slot
    seg(lay, w, sx1, y1, x1, y1)                     # south wall, east of slot
    seg(lay, w, x0, y0, x0, y1)
    seg(lay, w, x1, y0, x1, y1)
    # slot: open notch drawn inward
    seg(lay, w, sx0, y1, sx0, y1 - 0.6)
    seg(lay, w, sx1, y1, sx1, y1 - 0.6)
    seg(lay, w, sx0, y1 - 0.6, sx1, y1 - 0.6)
# pin-1 marker outside the SW corner
seg(SILK, 0.25, x0 - 0.45, y1 + 0.0, x0 - 0.45, y1 - 0.8)

c = 0.25
for a, bpt in (((x0 - c, y0 - c), (x1 + c, y0 - c)),
               ((x1 + c, y0 - c), (x1 + c, y1 + c)),
               ((x1 + c, y1 + c), (x0 - c, y1 + c)),
               ((x0 - c, y1 + c), (x0 - c, y0 - c))):
    seg(CRT, 0.05, a[0], a[1], bpt[0], bpt[1])

pcbnew.SaveBoard(BOARD, b)
print("saved")
