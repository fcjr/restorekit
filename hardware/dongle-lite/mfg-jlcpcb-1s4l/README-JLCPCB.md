# dongle-lite — single-sided 4-layer — JLCPCB fab package

LEFTSHIFT / LOGICAL USB-C DFU dongle. **RP2354A** MCU. **4-layer, single-sided
assembly** (all components on top) — for JLCPCB **Economic PCBA**.

## Files
| File | Use |
|------|-----|
| `dongle-lite-1s4l-gerbers-jlcpcb.zip` | Upload to **JLCPCB → PCB order** (gerbers + Excellon drill) |
| `cpl.csv` | Assembly: component placement (57 placements, all top) |
| `bom.csv` | Assembly: bill of materials (LCSC part #s) |
| `dongle-lite-1s4l.step` | 3D model (enclosure fit check) |

## Board specs
- **Size:** 22.1 × 77.1 mm
- **Layers:** 4, through-hole vias only (standard — NOT HDI)
- **Assembly:** **single-sided** — every one of the 57 assembled parts is on the
  top (F.Cu). Enables JLCPCB Economic PCBA (no second-side reflow).
- **Min trace/space:** 0.12 mm trace / 0.09 mm space — within JLCPCB standard tier
- **Vias:** uniform 0.45 mm pad / 0.20 mm drill (93 vias) — JLCPCB's standard
  4-layer via (0.125 mm annular ring), so **no small-hole / 4-wire Kelvin test
  upcharge**. **No via-in-pad** — all vias are dog-boned outside pads, so **no
  POFV upcharge** needed either.
- **Copper:** 1 oz outer / 0.5 oz inner (JLCPCB 4-layer default)
- **Surface finish:** ENIG recommended (0.4 mm-pitch QFN + USB-C)

## Stackup (4 signal layers, distributed ground)
This variant trades the dedicated internal ground *plane* for a 4th **routing**
layer — the density of the RP2354A's 0.4 mm-pitch escape needs it to route
single-sided. Ground is instead a **poured fill on all four layers**, tied
together through the signal vias and stitching.

| Position | Gerber | Function |
|----------|--------|----------|
| L1 (top) | `*-F_Cu.gtl` | signal + GND fill (all components here) |
| L2 | `*-GND1_Cu.g1` | signal + GND fill |
| L3 | `*-Sig2_Cu.g2` | signal + GND fill |
| L4 (bottom) | `*-B_Cu.gbl` | signal + GND fill |

When JLCPCB asks to confirm inner-layer order, map L2→`GND1_Cu`, L3→`Sig2_Cu`.

## Design-rule status
- **DRC: 0 errors, 0 unconnected nets.** All 51 signal nets route; ground is
  fully connected. Closest hole-to-hole spacing is 0.35 mm edge-to-edge (dense
  RP2354A / hub USB escape) — well within JLCPCB's standard capability.
- Remaining KiCad reports are cosmetic only: 6 silkscreen-over-copper and 6
  library-parity notes. None block fabrication.

## BOM notes
- `J3` (Tag-Connect TC2030) and `TP1–TP8` are bare pads — not assembled, excluded
  from `bom.csv` and `cpl.csv`.
- Two symbols previously had a stale LCSC part # (fixed in the schematic / BOM):
  - **C6, C7, C9** (4.7 µF) → `C368809` (Samsung CL05A475KP5NRNC)
  - **R12** (33 Ω) → `C25105` (UNI-ROYAL 0402WGF330JTCE)

## Signal integrity note
Because ground is a distributed pour (no solid reference plane), the USB 2.0
differential pairs are not over a continuous plane the way the 6-layer variant is.
USB 2.0 is tolerant of this; for the best SI, the **6-layer board remains the
reference build**. This single-sided variant exists to hit the Economic-PCBA cost
target.
