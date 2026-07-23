# RecoverKit Dongle Pro - Product Requirements Document

Status: Draft v0.1
Owner: Frank Chiarulli Jr.
Last updated: 2026-07-22

## 1. Summary

The Dongle Pro is the Dongle Lite (`../dongle-lite/PRD.md`) with a USB 3.1
Gen 1 (5 Gbps) SuperSpeed passthrough between the host and target ports. It is
Lite's platform — same RP2354A + FUSB302B DFU-trigger core, same SBU serial
block, same two-port bus-powered form — plus a USB 3 hub in the data path and
per-port SuperSpeed lane muxes.

Context that still applies: Apple DFU restore is hard-capped at USB 2.0 on the
target side (FB13555999), so the Pro does **not** restore faster than the Lite.
The SS path carries traffic when the connected target is a normally-booted
SuperSpeed device; DFU and restore continue to run over the hub's USB 2.0 path
exactly as on the Lite.

## 2. Delta over Lite

| | Dongle Lite | Dongle Pro |
|---|---|---|
| Receptacles | 16-pin HRO TYPE-C-31-M-12 (C165948) | 24-pin SHOU HAN TYPE-C 24P QT (C2681555), full-featured |
| Hub | CH334F USB 2.0 | GL3510-OSY52 USB 3.1 Gen 1 (C7501408, QFN-64) |
| Host CC | 2x 5.1k Rd pulldowns | HD3SS3220RNHR (C165155): UFP-mode CC controller + SS 2:1 mux |
| Target SS mux | — | HD3SS3212IRKSR (C544517), data-only; SEL driven by MCU GP26 |
| SS ESD | — | 4x TPD4E05U06DQAR (C138714), flow-through, one per lane |
| Hub clock | 12 MHz | 25 MHz +/-300ppm (X322525MOB4SI, C9006) |
| Hub power | 3.3 V from board rail | GL3510 integrated 3.3 V LDO + 1.2 V buck (second AOTA 3.3uH) |
| iProduct / serial | `Dongle-Lite` / `DL-` | `Dongle-Pro` / `DP-` |
| Firmware | `dongle-lite-fw` | same crate, `--features pro` |
| Update asset | `dongle-lite-fw.bin` | `dongle-pro-fw.bin` (same release tag) |

## 3. The two-CC-owners rule

The HD3SS3220 owns the **host** port CC autonomously (UFP mode, PORT=GND, no
I2C) — it replaces the Lite's discrete Rd pulldowns and steers its own SS mux
on plug flip. The FUSB302B remains the **only** owner of the target port CC:
the Apple DFU VDM flow is untouched. The target-side HD3SS3212 never touches
CC — it is a data-only lane mux whose SEL pin the firmware drives from the
FUSB302B's reported orientation.

Channel wiring (layout-driven): mux channel **C** is the target's
normal-orientation lane (CC1), channel **B** the flipped lane (CC2). The chip
selects A<->B at SEL=low, A<->C at SEL=high, so firmware drives
**SEL = high for CC1/normal, low for CC2/flipped** (`ss_sel` in
`crates/dongle-lite-fw`). A 10k pulldown keeps the flipped lane selected until
PD connects.

## 4. Architecture

```
 Host USB-C 24p --CC--> HD3SS3220 (UFP, autonomous)
      |  SS(2 lanes) --> HD3SS3220 mux --1 lane--> GL3510 USPORT
      |  D+/D- --------------------------------------> GL3510 USPORT (USB2)
                        GL3510 DS1 (USB2) ----------> RP2354A  (CDC0/CDC1/vendor)
                        GL3510 DS2 (SS+USB2) -+-SS--> HD3SS3212 --2 lanes--> Target USB-C 24p
                                              +-D+/D- ---------------------> Target D+/D-
 FUSB302B --CC--> Target CC (Apple DFU VDM, unchanged from Lite)
 SBU serial block (74AVC1T45 x2 + 1.2V rail): unchanged from Lite
```

- GL3510-OSY52 is the 4-DFP die: DS1 = MCU (USB 2.0 only), DS2 = target
  (SS + USB 2.0), DS3 unconnected, DS4 strap-disabled (FN_B). The host sees one
  permanently-empty hub port — cosmetic only.
- All SS TX links are AC-coupled once per segment (100 nF 0402) on the
  transmitter side of each hop, per USB 3.x + TI HD3SS3220 Fig 7-3.
- GL3510 support: 25 MHz crystal, 20 k 1% RTERM, RESETJ + VBUS sense dividers
  (47k/100k from +5 V, ~3.4 V), FN_B + PLED/FN_C 10 k straps, 1.2 V buck via a
  second AOTA 3.3 uH. RESETJ is also on MCU GP27 (`HUB_RSTn`, drive low to
  force hub re-enumeration; leave Hi-Z otherwise).
- HD3SS3220 sequencing: VDD5 must be stable >=2 ms before VCC33
  (t_VDD5V_PG), guaranteed by an RC delay (100k/100nF) on the RT9013's EN pin.
- Cable note: SS passthrough needs full-featured cables on **both** ports; with
  a USB 2.0 cable the link auto-degrades and DFU/restore still work.

## 5. Board

- 26 x 98 mm, 4-layer JLC04161H-7628, single-sided assembly (Economic PCBA).
- Stackup: F.Cu signal / In1 solid GND (unbroken under the whole SS corridor)
  / In2 crossunders + general routing / B.Cu routing + pour. Unlike the Lite
  1s4l's distributed-fill compromise, In1 **must stay solid** here — 5 Gbps
  does not tolerate plane splits under the pairs.
- 90 ohm differential: 0.211 mm width / 0.127 mm gap on outer layers
  (JLCPCB impedance calculator, JLC04161H-7628). Order with the impedance
  control option. USB 2.0 pairs keep the Lite geometry.
- USB-C fanout: the receptacle's flipped-side pairs arrive polarity-inverted
  at the muxes (inherent to Type-C rotational symmetry); each affected pair
  takes a short In2/B.Cu crossunder with GND1 as reference and stitching vias
  nearby. Chip-side detail lives in `gen/route.py`.
- Everything is generated: `gen/board.py` (netlist -> schematic),
  `gen/pcb.py` (placement), `gen/route.py` (hand-routed SS + D+/-),
  freerouting + `gen/ses.py` (remaining nets), `gen/zones.py` (GND pours +
  stitching). See README for the exact regen sequence.

## 6. Identification and updates

Per `restorekit-dongle-proto`: shared VID:PID 16D0:14F0; iProduct
`Dongle-Pro`; serial prefix `DP-`. Host SDK maps it to `DongleModel::Pro`.
Firmware is the Lite crate built with `--features pro`; releases publish both
images under one `dongle-lite-v*` tag and the updater selects
`dongle-pro-fw.bin` by detected model, so cross-model flashes can't happen.

## 7. Bring-up additions over Lite

1. HUB_3V3 / HUB_1V2 rails up (test points provided) before USB.
2. Hub enumerates SuperSpeed on the host (`lsusb -t` shows 5000M).
3. MCU enumerates behind DS1; DFU trigger + restore regression (USB 2.0 path).
4. SS passthrough to a booted SS device, both target-cable orientations
   (SEL follows polarity), both host-cable orientations (HD3SS3220 autonomous).
5. SBU serial regression.

Known risks (in order): GL3510 external values are inferred from GL3523
reference practice (no public GL3510 reference schematic) — RESETJ/VBUS levels
and buck inductor are the specific unknowns, all reworkable 0402s; 0.4 mm-pitch
QFN-64 + USON-10 assembly yield (ENIG + CPL rotation audit); SS impedance on
the standard 4-layer stackup. Intra-pair skew is length-tuned (gen/tune.py
serpentines every pair above 0.25 mm mismatch down to <0.2 mm, ~1.3 ps); the
short mux-to-hub links retain up to ~0.8 mm (~5 ps), inside the Gen 1 budget.
