# dongle-lite — JLCPCB fab package

LEFTSHIFT / LOGICAL USB-C DFU dongle. **RP2354A** MCU (internal flash), 6-layer,
through-via (no HDI).

## Files
| File | Use |
|------|-----|
| `dongle-lite-gerbers-jlcpcb.zip` | Upload to **JLCPCB → PCB order** (gerbers + Excellon drill) |
| `cpl.csv` | Assembly: component placement (63 placements) |
| `bom.csv` | Assembly: bill of materials (LCSC part #s — see the two CHECK items below) |
| `dongle-lite.step` | 3D model (enclosure fit check) |

## Board specs
- **Size:** 16.1 × 55.1 mm
- **Layers:** 6, through-hole vias only (standard — NOT HDI)
- **Thickness:** 1.6 mm (JLCPCB default 6-layer)
- **Min trace/space:** 0.09 mm (3.5 mil) — select JLCPCB's fine tier
- **Min via:** 0.30 mm pad / 0.15 mm drill
- **Copper:** 1 oz outer / 0.5 oz inner (JLCPCB 6-layer default)
- **Surface finish:** ENIG recommended (fine-pitch QFN + USB-C + via-in-pad)

## ⚠️ Via-in-pad (REQUIRED order option)
The RP2354A's 0.4 mm-pitch QFN and several ground pads are escaped with
**via-in-pad** (72 vias land inside SMD pads). When ordering you **must** enable
JLCPCB's **"Via in pad / POFV"** option (epoxy-filled + capped/plated-over vias).
Without it the vias will wick solder and starve the joints. This is an upcharge —
budget for it.

## Stackup / inner-layer order (IMPORTANT for 6-layer upload)
JLCPCB will ask you to confirm the inner-layer order. Map them top→bottom:

| Position | Gerber | Function |
|----------|--------|----------|
| L1 (top) | `*-F_Cu.gbr` | signal |
| L2 | `*-GND1_Cu.gbr` | **GND plane** |
| L3 | `*-Sig2_Cu.gbr` | signal |
| L4 | `*-Sig3_Cu.gbr` | signal |
| L5 | `*-PWR4_Cu.gbr` | **+3V3 plane** |
| L6 (bottom) | `*-B_Cu.gbr` | signal |

## Design-rule status
- **DRC: 0 errors, 0 unconnected nets.** Copper, clearance, and connectivity are clean.
- Ground uses a **solid (non-thermal) plane connection** — good for grounding,
  standard for reflow assembly.
- Remaining KiCad reports are cosmetic only: silkscreen-over-copper /
  reference-designator overlap and a couple of same-net (+3V3) via spacings.
  None block fabrication.

## BOM notes
- Previously two symbols had a stale LCSC part # (from an old value); **fixed** in
  the schematic and this BOM:
  - **C6, C7, C9** (4.7 µF) → `C368809` (Samsung CL05A475KP5NRNC, 4.7 µF 10 V X5R 0402)
  - **R12** (33 Ω) → `C25105` (UNI-ROYAL 0402WGF330JTCE, 33 Ω 0402)
- `J3` (Tag-Connect TC2030) and `TP1–TP8` are bare pads — not assembled, and are
  excluded from both `bom.csv` and `cpl.csv`.
- Minor cosmetic-only note: the PCB's *value text* for **C7** still reads `100nF`
  while the schematic (and the assembled part) is `4.7uF`. It's a silkscreen field,
  not a part change — does not affect the order. Re-run "Update PCB from Schematic"
  to sync the label if you care.

## Impedance
USB 2.0 differential pairs (host / target / MCU↔hub) are routed as pairs over the
GND plane (L2) but are **not** formally impedance-controlled. USB 2.0 is tolerant;
for a spin with controlled 90 Ω diff impedance, request a JLCPCB stackup quote.

## Assembly (optional)
Upload `bom.csv` + `cpl.csv` in JLCPCB's assembly step. Most parts have LCSC
part numbers; verify stock and confirm the RP2354A / CH334F / FUSB302 placements
(and the two BOM CHECK items above) before committing.
