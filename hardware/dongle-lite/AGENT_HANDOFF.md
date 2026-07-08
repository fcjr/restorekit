# Handoff: build RecoverKit Dongle Lite v1

You are picking up hardware design for the **RecoverKit Dongle Lite**, a small
USB-C board that forces an Apple Silicon / T2 Mac into DFU mode over USB-PD and
hands it to a host for restore. It removes restorekit's "automatic DFU only
works on macOS" limitation so restore works from Windows and Linux too.

## Read first

- `hardware/dongle-lite/PRD.md` - the full spec for v1 (Host + Target, no target
  power). This is your source of truth.
- `hardware/dongle/PRD.md` - the later powered variant. Do NOT build this yet;
  just keep v1 compatible with it as the base platform.

## What v1 is

Two USB-C ports, bus-powered from the host:
- Host port -> USB 2.0 hub -> RP2040 (control + serial) and the target's D+/D-.
- Target port -> Mac being recovered. The dongle owns its CC line and sends the
  Apple DFU VDM. SBU1/SBU2 carry the Mac's 1.2 V serial console back to the MCU.
- Host sees three USB things on one cable: RP2040 control CDC, RP2040 target-
  serial CDC, and the target Mac in DFU mode.

Firmware is an RP2040 port of AsahiLinux vdmtool / Central Scrutinizer.
Key Apple VDMs (SVID 0x05AC): DFU trigger `5AC8012,0106,80010000`; serial-UART
mux `0x5AC8012, 0x01840306`. Orientation from FUSB302 CC compare (`if cc1 > cc2`).

## Hard constraints

- EDA: **KiCad** (latest stable). Project lives in `hardware/dongle-lite/`.
- Parts: **LCSC only**, every line must map to an in-stock LCSC part.
- Assembly: **JLCPCB** SMT. Prefer Basic parts. Produce JLCPCB BOM + CPL.
- USB 2.0 only (no SuperSpeed/TB routing). Route D+/D-, CC, and SBU sideband.
- Commits/PRs: this is a public repo. Write commit messages as a human dev
  would. No AI/tool attribution, no Co-Authored-By.

## Reference designs (crib these, don't reinvent)

- Central Scrutinizer (SBU block + JLCPCB-verified BOM):
  https://git.kernel.org/pub/scm/linux/kernel/git/maz/cs-hw.git and
  https://git.kernel.org/pub/scm/linux/kernel/git/maz/cs-sw.git
- Tamarin-C (KiCad SBU layout to port):
  https://github.com/stacksmashing/tamarin-c-hw
- AltmodeFriend (bare RP2040 + FUSB302 core):
  https://github.com/CRImier/MyKiCad/tree/master/Peripherals/altmode_friend
- vdmtool (firmware/protocol): https://github.com/AsahiLinux/vdmtool

## Anchored BOM (already resolved; verify stock at order time)

- FUSB302BMPX -> LCSC C132291
- Level translators 74AVC1T45GW x2 -> C282330
- 1.2 V LDO TLV70212 -> C81462
- USB-C receptacle HRO TYPE-C-31-M-12 (16-pin, has SBU) -> C165948
- ESD USBLC6-2SC6 -> C7519
- RP2040 -> C2040; W25Q32 flash; 12 MHz 3225 crystal + 22 pF; 22R USB series R
- Still to pick: USB 2.0 hub (CH334F ~C425949, verify), 3.3 V LDO (AMS1117-3.3
  C6186 or better)

Our only net-new work over the references: the USB 2.0 hub and onboard 3.3 V
from host 5 V. Everything else is a port.

## Start here (in order)

1. **M0 bench proof before any layout.** On an FUSB302B breakout + a Pi Pico,
   run/port vdmtool (or Central Scrutinizer firmware). Confirm against a real
   target Mac: (a) DFU trigger works, (b) 1.2 V SBU serial works in BOTH cable
   orientations. The 1.2 V level shift + orientation swap is the highest-risk
   item; prove it here. Report results before moving on.
2. **Scaffold the KiCad project** in `hardware/dongle-lite/` as hierarchical
   sheets: `core` (RP2040 + flash + crystal, from AltmodeFriend), `pd-serial`
   (FUSB302 + 74AVC1T45 x2 + TLV70212 + connectors, from Central Scrutinizer /
   Tamarin-C), `hub-power` (USB 2.0 hub + 3.3 V LDO + target VBUS load switch).
3. Draft the schematic, then map every symbol to an LCSC part.
4. Layout (4-layer, controlled-impedance 90 ohm USB 2.0 pair), JLCPCB DRC clean,
   generate fab + assembly files.

## DFM cautions inherited from Central Scrutinizer

- Its tiny USB analog switch hit ~80% solder-defect at JLCPCB. If you add any
  analog mux, pick a larger, reliably-solderable package.
- Double-check CPL rotation offsets on small parts before ordering.

## Working style

- Confirm the M0 bench result before committing to a PCB.
- Keep v1 a strict subset of the powered dongle's platform so the power variant
  drops in later.
- Ask before ordering anything that costs money.
