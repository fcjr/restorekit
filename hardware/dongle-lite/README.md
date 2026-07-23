# Dongle-Lite — KiCad hardware

The USB 2.0 variant of the RecoverKit dongle: an RP2354A + FUSB302B board that
forces an Apple Silicon / T2 Mac into DFU over USB-PD, no keyboard dance
required. See [`../README.md`](../README.md) for how the dongle family works
and how the Lite compares to the [Dongle-Pro](../dongle-pro/), `./PRD.md` for
the product spec, and `./BOM-sourcing.md` for parts sourcing.

## How it works (short version)

The host plugs into J1, the target Mac into J2. A CH334F USB 2.0 hub puts two
devices on the host's bus: the RP2354A (control + CDC serial for `restorekit`)
and the target itself. The FUSB302B drives the *target* CC line with Apple's
vendor-defined USB-PD messages to flip the Mac into DFU; an AP22653 load
switch power-cycles target VBUS for clean re-enumeration; two 74AVC1T45s
break out the target's SBU debug UART at 1.2 V. Restore data flows host ↔
target straight through the hub — the MCU only triggers, never proxies.

## Status

- **Schematic:** complete, single sheet, functionally clustered, ERC 0.
- **Layout:** two builds of the same netlist:
  - `dongle-lite.kicad_pcb` — original two-sided 4-layer layout
    (`mfg-jlcpcb/`).
  - `dongle-lite-1s4l.kicad_pcb` — **single-sided** 4-layer relayout for
    JLCPCB Economic PCBA (`mfg-jlcpcb-1s4l/`, DRC report checked in). This is
    the one to order.
- **Firmware:** `../../crates/dongle-lite-fw` (Embassy, `rp235xa`). The GPIO
  map lives in the firmware source and matches `gen/board.py`.

## Clusters

One flat sheet (`dongle-lite.kicad_sch`), five functional clusters — parts
that talk to each other are drawn wired together, only cross-cluster signals
use labels:

| Cluster | Contents |
|---------|----------|
| MCU core | RP2354A (internal stacked flash — no external QSPI part), 12 MHz crystal, BOOT button, Tag-Connect J3 |
| USB-PD | FUSB302B, I²C pull-ups, target CC |
| USB hub + ports | CH334F hub, both 16-pin USB-C receptacles, ESD arrays, host CC pull-downs |
| SBU serial | 2× 74AVC1T45 1.2 V level translators |
| Power | 3.3 V + 1.2 V LDOs, target-VBUS load switch, LEDs |

USB D+/D− pairs are impedance-controlled differential pairs on the PCB.

## How this was built

Every part is a real LCSC component. Symbols, footprints, and 3D models were
pulled with [`easyeda2kicad`](https://github.com/uPesy/easyeda2kicad.py) into
the project-local libraries under `lib/` (shared with the Pro via its `lib`
symlink); `sym-lib-table` / `fp-lib-table` register them via `${KIPRJMOD}`.

The schematic is generated, not hand-drawn: `gen/board.py` is the
authoritative netlist (every part → cluster placement, every pin → net) and
`gen/gen.py` emits the `.kicad_sch`. Regenerate:

```sh
cd hardware/dongle-lite
python3 gen/board.py            # writes dongle-lite.kicad_sch
kicad-cli sch erc dongle-lite.kicad_sch
```

## Key parts (all LCSC)

| Ref | Part | LCSC | Function |
|-----|------|------|----------|
| U1 | RP2354A | C41378174 | MCU (USB-native composite; 2 MB internal flash) |
| U2 | FUSB302BMPX | C132291 | USB-PD PHY, drives target CC (Apple DFU VDM) |
| U3 | CH334F | C5187527 | USB 2.0 hub — host sees MCU + target |
| U5 | RT9013-33GB | C47773 | 3.3 V LDO from host 5 V |
| U6 | TLV70212 | C81462 | 1.2 V LDO for the SBU translator low side |
| U8, U9 | 74AVC1T45GW | C282330 | SBU1/SBU2 level translators (3.3 ↔ 1.2 V) |
| U10 | AP22653 | C2158037 | Target-VBUS load switch (current-limited) |
| D10, D11 | USBLC6-2SC6 | C7519 | ESD arrays on host / target D± |
| J1, J2 | HRO TYPE-C-31-M-12 | C165948 | Host / target USB-C (16-pin) |
| Y1, Y2 | 12 MHz 3225 | C9002 | MCU + hub crystals |

Full BOM/CPL for ordering are in the `mfg-jlcpcb*/` packages.

## ERC configuration

`pin_not_driven`, `pin_to_pin`, `lib_symbol_issues`, and `lib_symbol_mismatch`
are set to *ignore* in the project files: the LCSC-generated symbols type
passive/IO pins as "Input/Unspecified", not real disconnections. Everything
else keeps default severity.
