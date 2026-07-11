# Dongle-Proto-Lite — KiCad hardware

Schematic for the RP2040 + FUSB302B USB-PD dongle that forces an Apple Silicon /
T2 Mac into DFU over USB-PD. See `../dongle-lite/PRD.md` for the product spec and
`../dongle-lite/BOM-sourcing.md` for the parts sourcing.

## Status

- **Schematic:** complete, **single sheet, functionally clustered**, ERC 0 errors
  (2 intentional warnings — the `SWDIO`/`SWCLK` bring-up nets have no header yet).
  Connectivity verified by net-partition against the design intent: no merges, no splits.
- **Layout:** not started.

## Sheet layout

One flat sheet (`dongle-lite.kicad_sch`), arranged as five functional clusters.
Parts that talk to each other are drawn wired together; power rails use power-port
symbols; only signals that cross between clusters use global labels — so the page
reads as connected blocks, not a wall of net labels.

| Cluster | Contents |
|---------|----------|
| MCU core | RP2040, QSPI flash, 12 MHz crystal, boot button |
| USB-PD | FUSB302B, I²C pull-ups, target CC |
| USB hub + ports | CH334F hub, both USB-C receptacles, ESD arrays, host CC pull-downs |
| SBU serial | 2× 74AVC1T45 1.2 V level translators |
| Power | 3.3 V + 1.2 V LDOs, target-VBUS load switch, LEDs |

USB D+/D− pairs are drawn as labeled nets, not wires — they are impedance-controlled
differential pairs routed on the PCB. Full pin-by-pin connections are in
[`WIRING.md`](WIRING.md).

## How this was built

Every part is a real LCSC component. Symbols, footprints, and 3D models were
pulled from LCSC/EasyEDA with [`easyeda2kicad`](https://github.com/uPesy/easyeda2kicad.py)
into the project-local libraries under `lib/` (so JLCPCB assembly footprints,
LCSC part numbers, and STEP/WRL models travel with the repo):

- `lib/dongle-lite.kicad_sym` · `lib/dongle-lite.pretty/` · `lib/dongle-lite.3dshapes/`
- `sym-lib-table` / `fp-lib-table` register them via `${KIPRJMOD}`

The schematic is generated from a netlist, not hand-drawn. `gen/board.py` is the
**authoritative netlist** (every part → cluster placement, every pin → net);
`gen/gen.py` (`build_wired`) emits the single `.kicad_sch` — rails to power ports,
in-cluster nets to breakout-routed wires, cross-cluster signals to labels.
Regenerate:

```sh
cd hardware/dongle-lite
python3 gen/board.py            # writes dongle-lite.kicad_sch
kicad-cli sch erc dongle-lite.kicad_sch
```

## BOM (all LCSC)

| Ref | Part | LCSC | Function |
|-----|------|------|----------|
| U1 | RP2040 | C2040 | MCU (USB-native composite: control + target serial) |
| U2 | FUSB302BMPX | C132291 | USB-PD PHY, drives target CC (Apple DFU VDM) |
| U3 | CH334F | C5187527 | USB 2.0 hub — host sees MCU + target as 2 devices |
| U4 | W25Q32JVSSIQ | C2834491 | QSPI boot flash |
| U5 | RT9013-33GB | C47773 | 3.3 V LDO from host 5 V |
| U6 | TLV70212 | C81462 | 1.2 V LDO for the SBU translator low side |
| U8, U9 | 74AVC1T45GW | C282330 | SBU1/SBU2 level translators (3.3 V ↔ 1.2 V) |
| U10 | AP22653 | C2158037 | Target-VBUS load switch (current-limited vSafe5V) |
| D10, D11 | USBLC6-2SC6 | C7519 | ESD arrays on host / target D+/D- |
| J1, J2 | HRO TYPE-C-31-M-12 | C165948 | Host / target USB-C (16-pin, breaks out SBU+CC) |
| Y1, Y2 | 12 MHz 3225 | C9002 | RP2040 + hub crystals |
| D1, D2 | KT-0603R LED | C2286 | power / status |
| SW1 | TS-1187A | C318884 | BOOTSEL button |

Passives: 5.1k (C25905), 4.7k (C25900), 1k (C11702), 10k (C25744), 47k
(C25819); 100nF (C1525), 1µF (C52923), 22pF (C1804), 10µF (C15850).

## MCU note

RP2040, matching the firmware in `../../crates/dongle-lite-fw` (`embassy-rp`, RP2040
boot2) — no chip retarget needed. GPIO map:

`GPIO10/11` SBU1/2 DIR · `GPIO12/13` SBU1/2 UART · `GPIO14` 1.2 V rail enable ·
`GPIO16/17` I²C SDA/SCL · `GPIO19` target-VBUS switch ON · `GPIO20` FUSB302
INT_N · `GPIO25` status LED · native USB → hub downstream port 1.

The RP2040 uses an internal LDO for the 1.1 V core (`VREG_VIN` 3.3 V →
`VREG_VOUT` → `DVDD`), so unlike the RP2350 there is **no external switching
inductor** — one fewer part.

## Confirm before layout

Connectivity is verified against the netlist, but these need a datasheet check
(LCSC auto-generated symbols don't carry the intent):

1. **RP2040 core supply** — internal LDO: `VREG_VIN` (3.3 V) → `VREG_VOUT` (1.1 V)
   → `DVDD` (pins 23, 50), decoupled with 1 µF + 100 nF. No external inductor.
2. **RP2040 supplies** — `IOVDD`, `USB_VDD`, `ADC_AVDD`, `VREG_VIN` tied to 3.3 V;
   `TESTEN` (pin 19) to GND. Confirm against the datasheet.
3. **CH334F strapping** — `PSELF`→GND, `RESET#`/`OVCUR#` pulled to the hub's
   internal `VDD33`; confirm polarity and whether the hub needs the external
   12 MHz crystal (Y2). `V5`=5 V in, `VDD33`=internal 3.3 V out (own rail).
4. **USBLC6-2SC6 pin pairs** — wired I/O1={1,6}, I/O2={3,4}, GND=2, VBUS=5.
5. **AP22653 ILIM (R8, 47k)** — sets the current limit; pick per the datasheet.
6. No series resistors on the USB data pairs (RP2040 USB is direct to the hub).

## ERC configuration

`pin_not_driven`, `pin_to_pin`, `lib_symbol_issues`, and `lib_symbol_mismatch`
are set to *ignore* in `dongle-lite.kicad_pro`. They fire only because the
LCSC-generated symbols type passive/IO pins as "Input/Unspecified" rather than
"passive" — not real disconnections. Everything else keeps default ERC severity.

## Next steps

- Add an SWD/bring-up header on `SWDIO`/`SWCLK` (+3V3/GND) — clears the 2 ERC
  warnings and satisfies PRD FR9.
- PCB layout: 4-layer, 90 Ω USB 2.0 differential pairs (PRD §9).
