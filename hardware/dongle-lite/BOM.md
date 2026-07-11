# Dongle Lite — Bill of Materials

Generated from the verified schematic netlist (`gen/board.py`), **RP2040 design rev A**. **54 placed parts, every line an in-stock LCSC part with a JLCPCB assembly footprint** (KiCad 10 project; `lib/` holds symbols/footprints/3D models). Re-check stock at order time; prefer JLCPCB **Basic** parts (no per-part feeder fee).

| # | Refs | Qty | Value | MPN | LCSC | Package | JLC | Notes |
|---|------|-----|-------|-----|------|---------|-----|-------|
| 1 | U1 | 1 | — | RP2040 | C2040 | QFN-56 7×7 | Extended | Dual-core MCU, USB-native → hub. Internal core LDO (no external inductor). Firmware already targets it. |
| 2 | U4 | 1 | — | W25Q32JVSSIQ | C2834491 | SOIC-8 | Basic | 32Mb QSPI boot flash. |
| 3 | SW1 | 1 | — | TS-1187A | C318884 | SMD tact | Basic | BOOTSEL. **Consider replacing with a QSPI_SS test pad** (see brief). |
| 4 | Y1,Y2 | 2 | — | X322512MSB4SI | C9002 | 3225 4-pad | Basic | 12MHz crystal. Y1=MCU, Y2=hub. + 22pF loads. |
| 5 | C1,C2,C15,C16 | 4 | 22pF | CL21C220JBANNNC | C1804 | 0805 | Basic | Crystal load cap 22pF (0805 — can drop to 0402). |
| 6 | R1,R5,R9 | 3 | 10k | 0402WGF1002TCE | C25744 | 0402 | Basic | 10k — RUN / hub reset / fault pull-ups. |
| 7 | C4,C6,C14,C18,C23,C24,C26 | 7 | 1µF | CL05A105KA5NQNC | C52923 | 0402 | Basic | Decoupling 1µF (incl. RP2040 core LDO out C4). |
| 8 | C5,C7,C8,C9,C10,C11,C12,C13,C17,C19,C20,C21,C25 | 13 | 100nF | CL05B104KO5NNNC | C1525 | 0402 | Basic | Decoupling 100nF. |
| 9 | U2 | 1 | — | FUSB302BMPX | C132291 | WQFN-14 2.5×2.5 | Extended | USB-PD CC PHY. Sources vSafe5V, runs Apple DFU/serial VDMs. |
| 10 | R2,R3,R4 | 3 | 4.7k | 0402WGF4701TCE | C25900 | 0402 | Basic | 4.7k — I²C + INT pull-ups. |
| 11 | U3 | 1 | — | CH334F | C5187527 | QFN-24 4×4 | Extended | USB2.0 hub (1↑/2↓). Host sees MCU + target as 2 devices. |
| 12 | J1,J2 | 2 | — | TYPE-C-31-M-12 | C165948 | USB-C 16P SMD | Extended | USB-C receptacle. J1=host, J2=target. Breaks out CC+SBU. |
| 13 | D10,D11 | 2 | — | USBLC6-2SC6 | C7519 | SOT-23-6 | Basic | USB D± ESD array, one per port. |
| 14 | R6,R7 | 2 | 5.1k | 0402WGF5101TCE | C25905 | 0402 | Basic | 5.1k — host CC Rd (present as device). |
| 15 | U8,U9 | 2 | — | 74AVC1T45GW | C282330 | TSSOP-6 | Basic | 1-bit dir level shifter (3.3V↔1.2V) per SBU line. |
| 16 | U5 | 1 | — | RT9013-33GB | C47773 | SOT-23-5 | Basic | 5V→3.3V LDO, 500mA, EN. Powers logic. |
| 17 | U6 | 1 | — | TLV70212 | C81462 | SOT-23-5 | Extended | 3.3V→1.2V LDO. SBU shifter low side. EN=SHIFT_EN. |
| 18 | U10 | 1 | — | AP22653W6-7 | C2158037 | SOT-23-6 | Extended | Target-VBUS load switch, adj. current limit (R8), reverse block. EN=TGT_VBUS_EN. |
| 19 | R8 | 1 | 47k | 0603WAF4702T5E | C25819 | 0603 | Basic | 47k — AP22653 ILIM set. **Set per current limit.** |
| 20 | D1,D2 | 2 | — | KT-0603R | C2286 | 0603 LED | Basic | D1 power / D2 status. |
| 21 | R10,R11 | 2 | 1k | 0402WGF1001TCE | C11702 | 0402 | Basic | 1k — LED series. |
| 22 | C22 | 1 | 10µF | CL21A106KAYNNNE | C15850 | 0805 | Basic | Bulk 10µF. |

**Total: 54 components across 22 lines.** Not placed: 7 `PWR_FLAG` markers (ERC-only).

## Sourcing rules
- **Every part must be orderable from LCSC and assemblable by JLCPCB** — no hand-sourced parts. If a listed LCSC code is out of stock at order time, substitute a pin/footprint-compatible LCSC part and note it.
- **Basic vs Extended:** Basic parts have no feeder fee. The ICs (RP2040, FUSB302, CH334F, TLV70212, AP22653, USB-C) are Extended — budget one feeder fee each. Passives are all Basic.
- **RP2040 vs RP2350:** RP2040 is cheaper and uses an internal LDO for the 1.1 V core, so the design needs **no external inductor** (the RP2350 buck inductor is removed).
- **R8 (47k)** sets the AP22653 current limit — recalc for the chosen VBUS limit before order.

