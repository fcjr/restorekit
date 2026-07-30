#!/usr/bin/env python3
"""Restore the six net routes lost in the polish3 cascade delete.
Geometry taken verbatim from layout/dongle-probe.ses (original DRC-clean session).
Additive only. KiCad python."""
import pcbnew

BOARD = "layout/layout.kicad_pcb"
b = pcbnew.LoadBoard(BOARD)
_nets = b.GetNetsByName()

def mm(v):
    return pcbnew.FromMM(v)

def P(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))

def track(netname, layer, pts, w=0.15):
    net = _nets[netname]
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(P(x1, y1)); t.SetEnd(P(x2, y2))
        t.SetWidth(mm(w)); t.SetLayer(layer); t.SetNet(net)
        b.Add(t)

def via(netname, x, y, dia=0.5):
    v = pcbnew.PCB_VIA(b)
    v.SetPosition(P(x, y))
    v.SetDrill(mm(0.3)); v.SetWidth(mm(dia))
    v.SetNet(_nets[netname]); v.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
    b.Add(v)

F, B = pcbnew.F_Cu, pcbnew.B_Cu

# PROBE_SWCLK: U1.4 -> under QFN edge -> B.Cu west -> F.Cu down the west edge -> R6.1
track("PROBE_SWCLK", F, [(104.42, 116.2126), (105.1986, 116.2126), (105.4607, 115.9505)])
via("PROBE_SWCLK", 105.4607, 115.9505)
track("PROBE_SWCLK", B, [(105.4607, 115.9505), (105.0914, 115.5812), (100.5664, 115.5812)])
via("PROBE_SWCLK", 100.5664, 115.5812)
track("PROBE_SWCLK", F, [(100.5664, 115.5812), (100.5664, 129.8488), (101.3294, 130.6118),
                         (102.1926, 130.6118), (103.19, 131.6092), (103.19, 132.7251)])

# PROBE_RESET: U1.9 -> west channel -> B.Cu diagonal between BOOT_SW and VREG_LX -> F.Cu through SW1 gap -> R8.1
track("PROBE_RESET", F, [(104.42, 118.2126), (103.7408, 118.2126), (103.7408, 120.8285), (103.3824, 121.1869)])
via("PROBE_RESET", 103.3824, 121.1869)
track("PROBE_RESET", B, [(103.3824, 121.1869), (103.3522, 121.2171), (108.8957, 126.7606), (108.8957, 127.9492)])
via("PROBE_RESET", 108.8957, 127.9492)
track("PROBE_RESET", F, [(108.8957, 127.9492), (108.8957, 131.2194), (107.39, 132.7251)])

# PROBE_BOOT: existing via at (105.409,118.342) -> B.Cu east lane -> down the east edge -> R11.1
track("PROBE_BOOT", B, [(105.4089, 118.3416), (106.1327, 117.6178), (114.7541, 117.6178),
                        (115.6327, 118.4964), (115.6327, 127.9494), (109.7371, 133.845)])
via("PROBE_BOOT", 109.7371, 133.845)
track("PROBE_BOOT", F, [(109.7371, 133.845), (109.49, 133.5979), (109.49, 132.7251)])

# XIN: U1.21 -> via -> B.Cu loop around XOUT/+1V1 walls -> via -> join C6.1 chain
track("XIN", F, [(107.2, 121.3826), (107.2, 120.7034), (107.3154, 120.7034), (108.094, 119.9248)])
via("XIN", 108.094, 119.9248)
track("XIN", B, [(108.094, 119.9248), (111.769, 123.5998), (111.769, 124.4939), (111.4228, 124.8401),
                 (110.9998, 124.8401), (110.6836, 124.5239), (108.2116, 124.5239), (108.0746, 124.6609)])
via("XIN", 108.0746, 124.6609)
track("XIN", F, [(108.0746, 124.6609), (107.2, 124.6609)])

# VBUS: D3.5 -> B.Cu lane at y=111.55 -> feed lane (gen_usb4 geometry)
track("VBUS", F, [(108.0, 110.541), (108.0, 111.55)])
via("VBUS", 108.0, 111.55, dia=0.6)
track("VBUS", B, [(108.0, 111.55), (105.0, 111.55)])
via("VBUS", 105.0, 111.55, dia=0.6)
track("VBUS", F, [(105.0, 111.55), (105.0, 109.95)])

# QSPI_SS: existing chain end -> via -> B.Cu diagonal -> via -> R5.1
track("QSPI_SS", F, [(103.7717, 113.2776), (103.7717, 113.9725)])
via("QSPI_SS", 103.7717, 113.9725)
track("QSPI_SS", B, [(103.7717, 113.9725), (104.1781, 113.9725), (105.8436, 115.638),
                     (108.5784, 115.638), (112.9832, 111.2332)])
via("QSPI_SS", 112.9832, 111.2332)
track("QSPI_SS", F, [(112.9832, 111.2332), (113.2401, 111.4901), (114.6, 111.4901)])

pcbnew.ZONE_FILLER(b).Fill(b.Zones())
pcbnew.SaveBoard(BOARD, b)
print("saved")
