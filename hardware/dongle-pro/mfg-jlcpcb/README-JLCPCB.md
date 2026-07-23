# Dongle-Pro — JLCPCB ordering notes

## PCB

- Upload `dongle-pro-gerbers-jlcpcb.zip`.
- 4-layer, 26 x 98 mm. **Stackup: JLC04161H-7628** (select it explicitly).
- **Impedance control: YES** — the board carries 5 Gbps USB 3.1 Gen 1 pairs.
  Spec: 90 ohm differential, 0.211 mm track / 0.127 mm gap, outer layers,
  reference plane = inner layer 1 (GND1). Pick the 7628-prepreg option in the
  impedance calculator; do not let them substitute a different stackup.
- Layer order in the zip: `F_Cu`, `GND1_Cu` (In1), `Sig2_Cu` (In2), `B_Cu`.
- Surface finish: **ENIG** recommended (0.4 mm-pitch QFN-64 + USON-10).
- Min drill in design: 0.15 mm (SS-region microvias are mechanical NPTH-free
  through vias; standard capability, no extra option needed).

## Assembly (Economic PCBA, single side)

- All components are on the **top side only**.
- `bom.csv` + `cpl.csv` are in JLC format.
- Not populated: TP1-TP11 (test points), J3 (Tag-Connect, no part).
- **CPL rotation audit:** after upload, visually verify in JLC's viewer:
  - U3 (HD3SS3220, WQFN-30) and U7 (HD3SS3212, DHVQFN-20) — pin-1 dot
    orientation; both are rotated parts.
  - U4 (GL3510, QFN-64) — pin 1 top-left at 0 deg.
  - J1/J2 (24-pin USB-C) — shell toward the board edge.
  - D12-D15 (TPD4E05U06, USON-10) — flow-through orientation, pin 1 marks.
- Y2 is the 25 MHz hub crystal (X322525MOB4SI, +/-30 ppm); Y1 is the 12 MHz
  MCU crystal. Do not merge the two lines even though the package matches.
