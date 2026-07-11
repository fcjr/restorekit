# Dongle Lite -- wiring / connection list

Generated from the verified schematic netlist (`gen/board.py`), **RP2040 design rev A**. Every net below was checked by full net-partition against design intent: **no merges, no splits**. USB D+/D- pairs are labeled nets -- route them as impedance-controlled differential pairs, not hand-drawn wires.

## Parts
| Ref | Part | LCSC | Function |
|-----|------|------|----------|
| D1 | LED 0603 | C2286 | power LED |
| D2 | LED 0603 | C2286 | status LED |
| D10 | USBLC6-2SC6 | C7519 | host USB ESD |
| D11 | USBLC6-2SC6 | C7519 | target USB ESD |
| J1 | USB-C 16p | C165948 | host port (to Mac) |
| J2 | USB-C 16p | C165948 | target port (to iDevice) |
| SW1 | tact sw | C318884 | BOOTSEL |
| U1 | RP2040 | C2040 | dual-core MCU (internal core LDO) |
| U2 | FUSB302BMPX | C132291 | USB-PD CC PHY |
| U3 | CH334F | C5187527 | USB 2.0 hub (1 up / 2 down) |
| U4 | W25Q32JV | C2834491 | 32Mb QSPI flash |
| U5 | RT9013-33 | C47773 | 5V->3V3 LDO |
| U6 | TLV70212 | C81462 | 3V3->1V2 LDO |
| U8 | 74AVC1T45 | C282330 | SBU1 level shifter |
| U9 | 74AVC1T45 | C282330 | SBU2 level shifter |
| U10 | AP22653 | C2158037 | VBUS load switch |
| Y1 | 12MHz xtal | C9002 | MCU clock |
| Y2 | 12MHz xtal | C9002 | hub clock |

Passives: R1 10k (RUN), R2/R3 4.7k (I2C), R4 4.7k (INT), R5 10k (hub reset), R6/R7 5.1k (host CC Rd), R8 47k (ILIM), R9 10k (fault), R10/R11 1k (LEDs); C1/C2/C15/C16 22pF (xtal load), C4 1uF (core LDO out), decoupling 100nF/1uF/10uF.

## Power tree
```
J1 VBUS -- +5V -+- U5 RT9013 ------------- +3V3 -+- U6 TLV70212 (EN=SHIFT_EN) -- +1V2
               |                                +- RP2040 internal LDO -------- +1V1 (core / DVDD)
               +- U10 AP22653 (EN=TGT_VBUS_EN) -- TGT_VBUS -- J2
```
The RP2040 core rail (+1V1) is its internal LDO (`VREG_VOUT` pin 45 -> `DVDD`), decoupled by C4/C5 -- no external inductor.

## Connections by net

### Power rails

| Net | Pins |
|-----|------|
| `+5V` | U3.19, J1.A4B9, J1.B4A9, D10.5, C17.1, U5.1, U5.3, U10.1, C22.1, C25.1, #FLG1.1 |
| `TGT_VBUS` | J2.A4B9, J2.B4A9, D11.5, U10.6, C26.1, #FLG7.1 |
| `+3V3` | U1.1, U1.10, U1.22, U1.33, U1.42, U1.49, U1.48, U1.43, U1.44, U4.8, R1.2, C6.1, C7.1, C8.1, C9.1, C10.1, C11.1, C12.1, U2.3, U2.4, R2.2, R3.2, R4.2, C13.1, C14.1, U8.1, U9.1, C20.1, U5.5, U6.1, R9.2, R10.1, C23.1, #FLG2.1 |
| `HUB_3V3` | U3.20, U3.1, R5.2, C18.1, C19.1, #FLG6.1 |
| `+1V2` | U8.6, U9.6, C21.1, U6.5, C24.1, #FLG3.1 |
| `+1V1` | U1.45, U1.23, U1.50, C4.1, C5.1, #FLG4.1 |
| `GND` | U1.57, U1.19, U4.4, SW1.3, SW1.4, Y1.2, Y1.4, C1.2, C2.2, C4.2, C5.2, C6.2, C7.2, C8.2, C9.2, C10.2, C11.2, C12.2, U2.8, U2.9, U2.15, C13.2, C14.2, U3.25, U3.18, J1.A1B12, J1.B1A12, J1.1, J1.2, J1.3, J1.4, J2.A1B12, J2.B1A12, J2.1, J2.2, J2.3, J2.4, D10.2, D11.2, Y2.2, Y2.4, C15.2, C16.2, R6.2, R7.2, C17.2, C18.2, C19.2, U8.2, U9.2, C20.2, C21.2, U5.2, U6.2, U10.2, R8.2, D1.2, D2.2, C22.2, C23.2, C24.2, C25.2, C26.2, #FLG5.1 |

### USB 2.0 data (differential pairs -- route as pairs)

| Net | Pins |
|-----|------|
| `HOST_DP` | U3.15, J1.A6, J1.B6, D10.1, D10.6 |
| `HOST_DM` | U3.14, J1.A7, J1.B7, D10.3, D10.4 |
| `MCU_DP` | U1.47, U3.12 |
| `MCU_DM` | U1.46, U3.11 |
| `TGT_DP` | U3.10, J2.A6, J2.B6, D11.1, D11.6 |
| `TGT_DM` | U3.9, J2.A7, J2.B7, D11.3, D11.4 |

### USB-C CC (PD)

| Net | Pins |
|-----|------|
| `HOST_CC1` | J1.A5, R6.1 |
| `HOST_CC2` | J1.B5, R7.1 |
| `TGT_CC1` | U2.10, U2.11, J2.A5 |
| `TGT_CC2` | U2.1, U2.14, J2.B5 |

### SBU serial + level-shift

| Net | Pins |
|-----|------|
| `SBU1_UART` | U1.15, U8.3 |
| `SBU2_UART` | U1.16, U9.3 |
| `SBU1_DIR` | U1.13, U8.5 |
| `SBU2_DIR` | U1.14, U9.5 |
| `TGT_SBU1` | J2.A8, U8.4 |
| `TGT_SBU2` | J2.B8, U9.4 |
| `SHIFT_EN` | U1.17, U6.3 |

### I2C + PD interrupt

| Net | Pins |
|-----|------|
| `I2C_SDA` | U1.27, U2.7, R2.1 |
| `I2C_SCL` | U1.28, U2.6, R3.1 |
| `FUSB_INT` | U1.31, U2.5, R4.1 |

### QSPI flash

| Net | Pins |
|-----|------|
| `QSPI_SS` | U1.56, U4.1, SW1.1, SW1.2 |
| `QSPI_SCLK` | U1.52, U4.6 |
| `QSPI_SD0` | U1.53, U4.5 |
| `QSPI_SD1` | U1.55, U4.2 |
| `QSPI_SD2` | U1.54, U4.3 |
| `QSPI_SD3` | U1.51, U4.7 |

### Clocks

| Net | Pins |
|-----|------|
| `XIN` | U1.20, Y1.1, C1.1 |
| `XOUT` | U1.21, Y1.3, C2.1 |
| `HUB_XI` | U3.4, Y2.1, C15.1 |
| `HUB_XO` | U3.3, Y2.3, C16.1 |

### Control / status

| Net | Pins |
|-----|------|
| `RUN` | U1.26, R1.1 |
| `SWDIO` | U1.25 |
| `SWCLK` | U1.24 |
| `HUB_RSTB` | U3.16, R5.1 |
| `TGT_VBUS_EN` | U1.30, U10.3 |
| `LED_STAT` | U1.37, R11.1 |
| `VBUS_FAULT` | U10.4, R9.1 |
| `ILIM_SET` | U10.5, R8.1 |
| `LEDP_A` | D1.1, R10.2 |
| `LEDS_A` | D2.1, R11.2 |

## Notes

- **USB pairs** (`*_DP`/`*_DM`) connect connector -> ESD array -> hub by net name; on the PCB route each pair together, length-matched, 90 ohm differential.
- **RP2040 core**: `VREG_VIN` (pin 44) = 3V3 in, `VREG_VOUT` (45) = 1.1V out -> `DVDD` (23, 50); no external inductor. `TESTEN` (19) tied to GND.
- **BOOTSEL**: `SW1` shorts `QSPI_SS` to GND at power-up. A blank board auto-enters the USB bootloader, and firmware exposes a `bootsel` command, so SW1 can be a test pad.
- **Host CC** `R6`/`R7` are 5.1k Rd -- the host port presents as a USB device to the Mac.
- **SBU** `U8`/`U9` shift the 1.2V target SBU lines to 3V3; `*_DIR` sets direction, `SHIFT_EN` powers the 1.2V low side.
