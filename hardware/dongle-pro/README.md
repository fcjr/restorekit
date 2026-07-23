# Dongle-Pro — KiCad hardware

The USB 3.1 Gen 1 (5 Gbps) passthrough variant of the
[Dongle-Lite](../dongle-lite/): same RP2354A + FUSB302B DFU-trigger platform,
plus a GL3510 USB 3 hub in the data path and per-port SuperSpeed lane muxes.
See [`../README.md`](../README.md) for the family overview (how the trigger
works, Lite vs Pro, why the Pro doesn't restore faster) and `./PRD.md` for
the product spec (delta over `../dongle-lite/PRD.md`).

## How it works (short version)

Same restore flow as the Lite — the FUSB302B VDM-triggers DFU on the target
CC and traffic runs host ↔ target through the hub. What the Pro adds is a
5 Gbps path for *booted* targets: an HD3SS3220 on the host port autonomously
handles CC and muxes the host's SS pairs into the GL3510; on the target port
an HD3SS3212 data-only mux steers the hub's SS lane onto whichever pair the
cable orientation demands, with firmware driving SEL from the FUSB302B's
reported polarity. DFU/restore itself stays on USB 2.0 (Apple's cap), so a
USB 2.0 cable degrades nothing that matters for recovery.

## Status

- **Schematic:** complete, generated, ERC 0 errors.
- **Layout:** complete, 26 x 98 mm 4-layer (JLC04161H-7628), single-sided
  assembly, DRC 0 errors. SuperSpeed pairs, the USB 2.0 D+/- paths, and the
  congested-region nets are hand-routed by `gen/route.py` / `gen/finish.py`;
  the remainder is autorouted (freerouting) and imported by `gen/ses.py`.
- **Fab:** JLCPCB package under `mfg-jlcpcb/`.

## Layer plan

| Layer | Use |
|-------|-----|
| F.Cu | components (single-sided), SS pairs, most signal |
| In1 (GND1) | **solid GND — never routed.** The 5 Gbps reference plane |
| In2 (Sig2) | SS polarity crossunders, USB-C B-row escapes, long corridors |
| B.Cu | secondary routing, D+/- detours, GND pour |

90 ohm SS differential: 0.211 mm / 0.127 mm gap (`USB3_SS` net class; JLCPCB
JLC04161H-7628 impedance control). USB-C's rotational symmetry makes two of the
four SS pairs per receptacle arrive polarity-inverted at the mux — those pairs
take short In2/B.Cu crossunders with GND stitching vias alongside.

## Library

Parts live in the Lite's project library, shared via the `lib` symlink
(`lib -> ../dongle-lite/lib`); `sym-lib-table` / `fp-lib-table` resolve it
through `${KIPRJMOD}`. New parts are pulled with `easyeda2kicad --full
--lcsc_id C... --output "$PWD/lib/dongle-lite"`.

## Regenerating the board

Everything is scripted; the checked-in `.kicad_sch`/`.kicad_pcb` are build
artifacts of this pipeline (KPY = KiCad's bundled python, needed for pcbnew):

```sh
cd hardware/dongle-pro
KPY=/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/Current/bin/python3
python3 gen/board.py            # netlist -> dongle-pro.kicad_sch
kicad-cli sch erc dongle-pro.kicad_sch
$KPY gen/pcb.py                 # placement -> dongle-pro.kicad_pcb (+ .kicad_pro via gen/project.py)
$KPY gen/route.py               # hand-routed SS pairs + D+/- (protected wiring)
$KPY gen/finish.py              # hand corridors for freerouting-resistant nets
# autoroute the rest: export DSN (mark wiring protected, GND1 as power layer),
# run freerouting-2.1.0 -de/-do -mt 1, then:
$KPY gen/ses.py                 # duplicate-aware SES import + board-edge cleanup
$KPY gen/zones.py               # GND pours on all 4 layers + stitching vias
$KPY gen/silk.py                # front labels + back artwork (idempotent)
kicad-cli pcb drc dongle-pro.kicad_pcb
$KPY gen/fab.py                 # mfg-jlcpcb/: gerbers, drill, BOM, CPL, STEP
```

`gen/project.py` rewrites `dongle-pro.kicad_pro` (net classes `USB3_SS`/`PWR`,
ERC ignores for LCSC symbol pin types) after every pcbnew save, because
`pcbnew.SaveBoard` clobbers the sibling project file.

## Firmware / identification

Same firmware crate as the Lite (`crates/dongle-lite-fw`) built with
`--features pro`: identifies as `Dongle-Pro`, serial `DP-…`, and drives the
HD3SS3212 lane select on GP26 (SEL = high for CC1/normal orientation — chip
channel C is the normal lane). GP27 is `HUB_RSTn` (drive low to force hub
re-enumeration). The host updater picks `dongle-pro-fw.bin` from the shared
`dongle-lite-v*` release tag by detected model.
