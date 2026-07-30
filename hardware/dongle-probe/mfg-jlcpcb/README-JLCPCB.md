# dongle-probe — JLCPCB fab package

SWD debug probe for the dongle family. **RP2354A** MCU (internal flash), USB-C,
TC2030-mating 2×3 2.54 mm IDC box header, target reset + BOOT control. 2-layer.

## Files
| File | Use |
|------|-----|
| `dongle-probe-gerbers-jlcpcb.zip` | Upload to **JLCPCB → PCB order** (gerbers + Excellon drill) |
| `cpl.csv` | Assembly: component placement (35 placements) |
| `bom.csv` | Assembly: bill of materials (all LCSC part #s verified in stock) |

## Board specs
- **Size:** 16 × 43.0 mm
- **Layers:** 2
- **Thickness:** 1.6 mm
- **Min trace/space:** 0.15 / 0.13 mm — standard tier, no upcharge
- **Min via:** 0.45 mm pad / 0.30 mm drill (three GND stitch vias at exactly
  0.45/0.30 — JLCPCB's stated 2-layer minimum)
- **Copper:** 1 oz
- **Surface finish:** ENIG recommended (0.4 mm-pitch QFN + USB-C), HASL workable

## Design-rule status
- **DRC: 0 unconnected items.** All nets routed, both GND pours fully stitched.
- **2 known clearance "errors" are internal to the J1 USB-C footprint** (the
  shell blade pads vs. their own GND tie tracks, 0.100 mm vs. the 0.130 mm rule).
  The same EasyEDA footprint shipped on dongle-lite unmodified. 0.100 mm is
  above JLCPCB's 2-layer minimum spacing (0.09 mm) — not a fab issue.
- 3 silkscreen-over-mask warnings — cosmetic only.

## Assembly notes
- **U1 (RP2354A, C41378174)** is an *Extended* part — small loading fee applies.
  Not stocked at LCSC retail but JLC assembly stock had ~1k units; re-verify
  stock before ordering.
- **J2 (C11214, BOOMELE 2.54-2*3P)** is a shrouded 2×3 2.54 mm through-hole
  IDC box header — the TC2030-IDC-NL cable ends in a 6-pin 0.1" IDC socket
  whose polarizing bump only seats one way. Solder it with its key slot toward
  the board edge (pin-1/3/5 row; the silkscreen notch marks it). Order with
  "Economic + hand-soldering of THT" or solder it yourself (6 joints). LCSC
  stock is deep (~30k); any generic DC3-6P box header also fits.
- **JP1** is a bare solder jumper (VTREF → +3V3), intentionally unpopulated:
  bridge it to power a target from the probe; leave open when the target
  self-powers.
- **R4** (RUN pull-up) is 5.1 kΩ (C25905, shared with R1/R2) — it was 10 kΩ
  C25744 until that part went short at JLC assembly. Any 1–10 kΩ works there;
  RP2350's RUN also has an internal pull-up.
- **Rotation check:** JLC's part orientation DB sometimes disagrees with KiCad
  for U1/U2/D3/J1. In the assembly preview, verify pin-1 of U1 (top-left dot),
  the SOT-23 parts, and the USB-C shell before confirming the order.
