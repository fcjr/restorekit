# RecoverKit Dongle Lite - Product Requirements Document

Status: Draft v0.1
Owner: Frank Chiarulli Jr.
Last updated: 2026-07-07

## 1. Summary

The RecoverKit Dongle Lite is a small two-port USB-C device that forces an Apple
Silicon (M series) or T2 Mac into DFU mode over USB Power Delivery, then hands
the target off to a host computer for restore. It exists to remove the hardware
limitation called out in the restorekit README: today automatic DFU entry only
works on macOS. With this dongle, restorekit can trigger DFU and restore a Mac
from Windows or Linux too, with no second Mac required.

Lite is the MVP. It does not supply meaningful power to the target: the target
runs on its own battery through the restore. A separate product, the RecoverKit
Dongle (see `../dongle/PRD.md`), adds a third port that can power the target
from an external USB-C PD charger. The full dongle is Lite plus a power
subsystem, so Lite defines the shared platform.

The dongle triggers DFU by sending Apple's vendor-defined USB-PD message on the
target's CC line. This mechanism is public and proven by the AsahiLinux
`vdmtool` project. The exact VDM that reboots a target into DFU is
`5AC8012,0106,80010000` (SVID `0x05AC` is Apple's USB vendor ID).

## 2. Goals

- Put a connected Apple Silicon or T2 Mac into DFU mode on command from a host,
  on macOS, Windows, and Linux.
- Present the target to the host as a normal DFU USB device so restorekit /
  Apple Configurator can restore it over USB 2.0.
- Be bus-powered from the host port. No wall power required.
- Expose the target's low-level serial console (AP/SEP UART over SBU) to the
  host as a second serial device, for debugging failed restores.
- Be the smallest, cheapest board that does the above, and serve as the shared
  platform the full dongle extends.
- Be fully buildable in KiCad, sourced only from LCSC, and assembled by JLCPCB.

## 3. Non-goals

- No target power beyond the minimal 5 V VBUS needed for PD signaling. Lite does
  not charge or sustain a dead-battery target. That is the full dongle's job.
- No third (power) port.
- No USB SuperSpeed or Thunderbolt data passthrough. DFU and restore run at USB
  2.0 High Speed (480 Mbps). We route only the USB 2.0 data pair. CC and SBU
  sideband are still routed, for PD/VDM and Apple serial respectively.
- No display, battery, or onboard storage.
- Not a certified retail product in v1 (USB-IF / FCC / CE out of scope).

## 4. Users and use cases

Primary user: an IT admin or individual restoring a Mac from a non-Mac host,
where the target has enough battery to complete a restore.

- UC1: Windows/Linux admin plugs host cable into their PC, target cable into a
  MacBook, runs restorekit. Dongle triggers DFU, restorekit restores.
- UC2: Developer debugging a restore that hangs opens the second serial device
  the dongle exposes and watches the target's AP console.

## 5. System requirements

### 5.1 Ports (two USB-C receptacles)

| Port | Role | Data | Power |
|------|------|------|-------|
| Host | Connects to the host PC running restorekit | USB 2.0 to control MCU + target | Sources 5 V into the dongle (bus power) |
| Target | Connects to the Mac being recovered | USB 2.0 D+/D-, CC (VDM), SBU (serial) | Receives 5 V for PD signaling only; dongle owns its CC |

Both ports use the cheap 16-pin `HRO TYPE-C-31-M-12` receptacle (LCSC C165948).
It breaks out SBU1/SBU2 as well as CC and D+/D-, so no 24-pin part is needed.
Tamarin-C uses exactly this connector for SBU serial, which confirms it.

### 5.2 Functional requirements

- FR1: On host command, negotiate a USB-PD contract with the target as its port
  partner and send the Apple DFU VDM. Target must enter DFU with no PD
  renegotiation.
- FR2: The dongle's PD controller owns the target CC line during trigger and
  restore.
- FR3: Present the target's USB 2.0 D+/D- to the host through a USB 2.0 hub, so
  the host sees the control MCU and the target as two independent devices on one
  cable.
- FR4: Provide only vSafe5V on the target VBUS, current-limited, sufficient for
  PD signaling. Do not attempt to power or charge the target.
- FR5: Support Apple serial mode. Send the vendor VDM that muxes the target's
  debug UART onto SBU1/SBU2, wire both SBU pins to the RP2040 through 1.2 V level
  shifting, and bridge the UART to a second host serial (CDC1) endpoint. See
  section 5.4.
- FR6: Serial mode is orientation-aware. The SBU pin on the same side as the
  active CC pin is the target's TX; the opposite SBU is RX. The dongle detects
  orientation from which CC line is active (the FUSB302B reports it) and assigns
  TX/RX accordingly, so the target cable works plugged in either way up.
- FR7: Also support debugusb mode (the kis-100000 device), which runs over the
  regular D+/D- pair already routed to the hub. No extra wiring needed, and it
  works with any cable.
- FR8: Status LEDs indicate power, PD-contract/DFU state, and target-serial
  activity.
- FR9: Expose an SWD header and test points for bring-up and firmware flashing.

### 5.3 Power requirements

- Dongle logic (MCU, hub, PD PHY, level shifter) is powered from host VBUS 5 V
  through a 3.3 V regulator. Total logic budget target < 250 mA at 5 V.
- Target VBUS: vSafe5V only, current-limited via a load switch. Signaling, not
  charging.

### 5.4 Apple serial mode (SBU)

Apple's low-level AP/SEP console is a UART muxed onto the target's SBU1/SBU2
pins after a vendor VDM enables it. This design mirrors the two open reference
boards that already solve it: the Central Scrutinizer (FUSB302 + a couple of
level shifters + an RP2040) and Tamarin-C (a KiCad board derived from it). We
port the Tamarin-C SBU block rather than reinvent it.

- Enabling the mux: firmware sends the Apple VDM `0x5AC8012, 0x01840306`
  ("mux debug UART over SBU1/2") after the PD contract is up.
- Logic level is ~1.2 V, below the 3.3 V the RP2040 speaks, so each SBU line
  passes through a single-bit level translator. Use the Central Scrutinizer v3.1
  choice: `74AVC1T45GW` (Nexperia, low side works down to 1.2 V), 2x, one per
  SBU line. Generate the 1.2 V low-side rail with a dedicated `TLV70212` 1.2 V
  LDO. Central Scrutinizer started with a 470/270 resistor divider (which
  Tamarin-C still uses) but moved to this LDO in v3.1 after the divider caused
  trouble, so prefer the LDO.
- Orientation swap, two proven approaches:
  - No mux (Tamarin-C): SBU1 -> translator -> a fixed RP2040 GPIO; SBU2 ->
    translator -> another GPIO. The FUSB302B reports polarity (as in `vdmtool`:
    `if (cc1 > cc2)` normal, else flipped). The RP2040 PIO then assigns UART TX
    to the active-CC-side GPIO and RX to the other, and sets each translator's
    DIR to match. Flipping the cable flips two firmware settings. Simplest BOM;
    recommended for our SBU-only serial.
  - Analog mux (Central Scrutinizer v3): an `RS2227XN` USB analog switch driven
    by a SEL_SBU GPIO physically swaps SBU1/SBU2 (and can also route serial onto
    D+/D-). CS needs this because it offers serial over D+/D-; we only take it on
    if we want that feature, and it adds parts and DFM risk (see section 9).
- Add ESD/TVS protection on the SBU (and D+/D-) lines at the connector.

Cable note: serial mode needs the SBU conductors, which only exist in full-
featured USB 3 / Thunderbolt USB-C cables. USB 2.0 and charge-only cables wire
just VBUS, GND, D+/D-, and CC, so they cannot carry serial. DFU trigger (CC/VDM)
and debugusb (D+/D-) work over any cable; only SBU serial needs a full cable on
the Target port. This is the same constraint the restorekit README notes.
Central Scrutinizer v3 sidesteps it by routing serial over D+/D- so cheap cables
work, but only when USB 2.0 passthrough is not needed at the same time. Our
dongle needs passthrough for restore, so we keep serial on SBU.

## 6. Architecture

```
 Host USB-C ---D+/D----> +-------------+ ---> RP2040  (CDC0: control, CDC1: target serial)
     (5V in)             |  USB 2.0    |         | I2C
                         |  hub        |         v
                         | (CH334)     |     FUSB302B (source) --CC--> Target CC   (VDM / PD)
                         +------+------+
                                +---D+/D------------------------------> Target D+/D-  (DFU/restore)

  Host 5V --> LDO 3.3V --> logic rails
  Host 5V --> load switch --> Target VBUS (vSafe5V, signaling only)
  Target SBU1/SBU2 --> 1.2V level shift (+ orientation swap) --> RP2040 UART   (AP console)
       ^ orientation (CC1/CC2 active) reported by FUSB302B decides TX vs RX
```

Design rationale for two settled questions:

- A USB 2.0 hub IS required. The host must see two USB devices on one cable: the
  control MCU and the target in DFU mode.
- FUSB302B (dumb PD PHY + MCU), NOT an integrated-MCU PD chip. The Apple DFU VDM
  sequence is already implemented on FUSB302B in open firmware (`vdmtool`). An
  integrated PD chip would mean reverse-engineering the Apple VDMs on a closed
  SDK.

## 7. Firmware

- MCU: RP2040. USB-native, enumerates directly to the host as a composite device
  (CDC0 control, CDC1 target serial). No separate USB bridge needed.
- Base the PD/VDM logic on a port of AsahiLinux `vdmtool` (AVR/Arduino) to
  RP2040 + FUSB302B over I2C, cross-referencing the Central Scrutinizer / Tamarin-C
  RP2040 firmware which already targets this exact stack.
- Key Apple VDMs (SVID `0x05AC`): DFU trigger `5AC8012,0106,80010000` (reboot to
  DFU, no PD renegotiation); serial-UART mux `0x5AC8012, 0x01840306`.
- Orientation: read CC1/CC2 from the FUSB302B, set polarity, and map SBU TX/RX
  to the active-CC side per FR6.
- Host command interface over CDC0: at minimum `dfu` (trigger), `reboot`,
  `serial` (remux SBU to UART), `status`. Line-oriented so restorekit can drive
  it.
- restorekit integration: restorekit opens CDC0, issues `dfu`, waits for the
  target to re-enumerate in DFU mode on the hub, then restores as it does today.
- The full dongle reuses this firmware unchanged and adds a power-subsystem
  driver on top.

## 8. Preliminary BOM (LCSC / JLCPCB)

All parts must be orderable from LCSC and assemblable by JLCPCB. Prefer JLCPCB
Basic parts. LCSC numbers are starting points and must be re-verified for stock
and JLC assembly at BOM finalization.

Legend: MPN is exact; the LCSC column is a starting point. Confident numbers are
given; the rest are marked "verify" and must be checked for stock and a JLCPCB
assembly footprint at M1. Prefer JLCPCB Basic parts to cut loading fees.

| Function | MPN | LCSC | Notes |
|----------|-----|------|-------|
| PD PHY, target (source) | FUSB302BMPX | C132291 | WQFN-14, I2C, in stock |
| MCU | RP2040 | C2040 | bare chip, per AltmodeFriend reference |
| QSPI flash | W25Q32JVSSIQ (SOIC-8) | verify | >= 2 MB boot flash, per AltmodeFriend |
| Crystal + load caps | 12 MHz 3225 4-pin + 2x 22 pF | verify | RP2040 XOSC |
| RP2040 USB series R | 22R x2 (0402) | Basic | on D+/D- up to the hub |
| Boot button | SMD tactile 4P | Basic | RP2040 BOOTSEL |
| USB 2.0 hub | CH334F (or FE1.1s) | C425949 (verify) | 1 upstream, 2 down |
| USB-C receptacle (Host + Target) | HRO TYPE-C-31-M-12 | C165948 | 16-pin, breaks out SBU + CC + D+/D- |
| SBU level translators | 74AVC1T45GW | C282330 | 2x (1 per SBU line), per CS v3 |
| 1.2 V rail | TLV70212 (1.2 V LDO) | C81462 | per CS v3.1 (divider is the older alt) |
| ESD array (per USB port) | USBLC6-2SC6 (subs Tamarin RClamp5524N) | C7519 | covers D+/D- and CC; add small TVS on SBU |
| Target VBUS load switch | TPS22918 or P-FET | verify | vSafe5V, current-limited |
| 3.3 V LDO | AMS1117-3.3 (or low-noise alt) | C6186 | from host 5 V (AltmodeFriend has none) |
| CC pulldowns / I2C pullups | 5.1k x2, 4.7k x3 | Basic | per Tamarin |
| Status LED | SK6805 or plain LEDs | Basic | + SWD header, test points |

Two open KiCad boards cover most of this design; we combine them:

- Core (bare RP2040 + FUSB302 over I2C): AltmodeFriend
  (github.com/CRImier/MyKiCad, Peripherals/altmode_friend). It is a bare-RP2040
  + FUSB302BMPX board with W25Q32 flash, 12 MHz 3225 crystal + 22 pF, 22R USB
  series resistors, boot button, and an SK6805 status LED, no onboard 3.3 V. Our
  RP2040/FUSB302/flash/crystal subcircuit follows it directly. Useful tip from
  its notes: the FUSB302 VBUS pin can be left unconnected without affecting PD.
- SBU serial block (level shift + orientation): Tamarin-C
  (github.com/stacksmashing/tamarin-c-hw): FUSB302BMPX, 5x SN74AVCH1T45DCKT,
  RClamp5524N ESD, HRO TYPE-C-31-M-12 connector, 470/270 divider for the 1.2 V
  rail, RPi Pico module. We drop to 2 translators (Tamarin's 5 also cover its
  SWD feature, which we do not need) and swap RClamp5524N for the JLC-basic
  USBLC6-2SC6.

Our net-new work over both references is the USB 2.0 hub, the onboard 3.3 V
regulation from host 5 V, and (on the full dongle) the power path.

### Central Scrutinizer v3.1 production BOM (JLCPCB-verified)

These LCSC numbers come from the cs-hw `production/bom.csv`, so they are known to
assemble at JLCPCB. This is the highest-confidence source for the SBU block.

| Ref | MPN | LCSC | Note |
|-----|-----|------|------|
| U1 | FUSB302BMPX | C132291 | PD PHY |
| U2, U3 | 74AVC1T45GW,125 | C282330 | level translators (2x) |
| U7 | TLV70212 (SOT-23-5) | C81462 | 1.2 V LDO for translator low side |
| U5, U6 | RS2227XN | C255478 | USB analog mux (only if doing D+/D- serial) |
| Q1 | AO3400A | C20917 | N-FET |
| J1 | USB-C (Molex 105450-0101) | C134092 | 24-pin receptacle CS uses (we use C165948) |
| C1, C2 | 470pF 0402 | C1537 | |
| C3-C6 | 0.1uF 0402 | C1525 | decoupling |
| C7, C8 | 1uF 0402 | C14445 | |
| R1,R2,R5,R6 | 4.7k 0402 | C25900 | pullups |
| R3, R4 | 10k 0402 | C25744 | |

Open BOM decisions:

- The SBU block is now resolved to CS v3.1 parts (all JLCPCB-verified above);
  no remaining "verify" items there. Still prototype the 1.2 V UART on the bench
  before layout, since it is the trickiest signal path.

## 9. Manufacturing constraints

- EDA: KiCad (latest stable). Project lives in this folder.
- Sourcing: LCSC only. Every BOM line must map to an in-stock LCSC part.
- Assembly: JLCPCB SMT. Provide JLCPCB-format BOM and CPL files. Prefer Basic
  parts; minimize Extended parts to control loading fees.
- Board: 4-layer recommended (signal / GND / power / signal) for clean USB 2.0
  impedance and a solid ground reference. Use JLCPCB's controlled-impedance
  option for the 90 ohm USB 2.0 differential pair.
- Design rules: JLCPCB standard capabilities. No blind/buried vias in v1.
- Single-sided placement preferred to halve assembly cost, if layout allows.
- DFM caution from Central Scrutinizer: its very small USB analog-switch package
  had an ~80% solder-defect rate at JLCPCB (fixed in v3.1 with a physically
  larger switch). If we use any USB analog mux (RS2227XN or similar), pick a
  larger, reliably-solderable package.
- Component rotation/orientation in the CPL is easy to get wrong on the small
  parts; CS's README stresses double-checking rotation offsets before ordering.

## 10. Bring-up and test plan

1. De-risk before layout: FUSB302B breakout + Raspberry Pi Pico, port `vdmtool`
   (or run Central Scrutinizer / Tamarin-C firmware), confirm DFU trigger, then
   1.2 V SBU serial in both cable orientations against a real target Mac. The
   1.2 V level shift and orientation swap are the highest-risk items, so prove
   them here before committing copper.
2. Board bring-up order: 3.3 V rail -> RP2040 + SWD flashing -> hub enumeration
   on host -> FUSB302B I2C + PD contract -> DFU VDM -> target enumerates in DFU
   on host -> restorekit restore end to end -> SBU serial remux.

## 11. Milestones

- M0: Bench proof with breakout + Pico (no custom PCB). Validates concept.
- M1: v1 schematic complete in KiCad, BOM mapped to LCSC.
- M2: v1 layout, JLCPCB DRC clean, fab + assembly files generated.
- M3: First article assembled at JLCPCB, bring-up per section 10.
- M4: restorekit CDC integration, cross-platform DFU + restore verified.

## 12. Open questions

- Confirm the target reliably enters DFU and completes a restore on battery
  alone with only vSafe5V present. If some targets need real power, users move
  up to the full dongle.
- Exact USB-C 24-pin receptacle with a confirmed JLCPCB assembly footprint.
- Enclosure: off-the-shelf vs custom. Out of scope for v1 electronics.

## 13. References

Prior-art designs this project is built on, and what we take from each:

- Central Scrutinizer (Marc Zyngier) - the upstream we lean on most. FUSB302 +
  level shifters + RP2040 serial adapter. Source of the SBU block and the
  JLCPCB-verified production BOM (see `production/bom.csv`; README revision notes
  document v3.1 = 1.2 V LDO + larger switches).
  - Hardware (KiCad): https://git.kernel.org/pub/scm/linux/kernel/git/maz/cs-hw.git
  - Firmware: https://git.kernel.org/pub/scm/linux/kernel/git/maz/cs-sw.git
  - Writeup: https://hackaday.io/project/192826-central-scrutinizer-a-serial-adapter-for-m1m2m3
- Tamarin-C (stacksmashing) - open KiCad board for this exact use case; we port
  its SBU level-shift + orientation block.
  - Firmware: https://github.com/stacksmashing/tamarin-c
  - Hardware (KiCad): https://github.com/stacksmashing/tamarin-c-hw
- AltmodeFriend (CRImier) - open KiCad bare-RP2040 + FUSB302BMPX board; our
  reference for the MCU/PD core.
  - Repo: https://github.com/CRImier/AltmodeFriend
  - KiCad: https://github.com/CRImier/MyKiCad/tree/master/Peripherals/altmode_friend
- AsahiLinux vdmtool - FUSB302 firmware with the DFU and serial-mux VDMs and CC
  orientation code; the basis for our firmware port.
  - https://github.com/AsahiLinux/vdmtool
- AsahiLinux macvdmtool - Mac-to-Mac VDM tool; source for the command set,
  1.2 V SBU serial notes, and cable/pin requirements.
  - https://github.com/AsahiLinux/macvdmtool
- Asahi Linux docs - protocol and wiring background:
  - USB-PD: https://asahilinux.org/docs/hw/soc/usb-pd/
  - Serial debug: https://asahilinux.org/docs/hw/soc/serial-debug/

The full dongle adds one more (power path): see `../dongle/PRD.md` section 12.
