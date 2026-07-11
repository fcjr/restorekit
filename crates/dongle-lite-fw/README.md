# Dongle Lite - M0 bench firmware

This is the **M0** milestone from `../PRD.md` section 11: prove, on a breadboard
against a real Apple Silicon / T2 Mac, that the two riskiest parts of the design
work before we commit any copper:

1. The Apple **DFU-trigger VDM** puts the target into DFU mode.
2. The **1.2 V SBU serial** console reads back cleanly in **both** cable
   orientations (the level-shift + orientation swap is the highest-risk item).

It runs on a stock Raspberry Pi Pico (RP2040) plus an FUSB302B breakout and two
74AVC1T45 level translators. No custom PCB. The firmware is Rust/Embassy and is
the seed of the production dongle firmware - the FUSB302 driver, the PD source
state machine, and the Apple VDMs all carry over unchanged; the product build
swaps the bench pin map for the real board's and adds the USB 2.0 hub path.

The PD/VDM logic is a port of AsahiLinux
[`vdmtool`](https://github.com/AsahiLinux/vdmtool) and Marc Zyngier's
[Central Scrutinizer](https://git.kernel.org/pub/scm/linux/kernel/git/maz/cs-sw.git).

## What you need

- Raspberry Pi Pico (RP2040). A Pico 2 (RP2350) would need a target tweak; use a
  Pico for M0.
- FUSB302B breakout (LCSC C132291 on the real board). Common breakouts:
  "FUSB302 USB-C PD" modules with SDA/SCL/INT/VBUS/GND broken out.
- 2x 74AVC1T45 single-bit level translators (LCSC C282330), one per SBU line.
- A 1.2 V source for the translators' low side. On the bench, a TLV70212
  (C81462) or any clean 1.2 V rail. Do **not** use the Pico 3V3 for the low side.
- A USB-C **breakout** that exposes SBU1/SBU2, CC1/CC2, D+/D-, VBUS, GND
  (e.g. a 24-pin USB-C test board). This is the **target** side, to the Mac.
- A **full-featured** USB-C cable (USB 3 / Thunderbolt). USB 2 / charge-only
  cables have no SBU wires and cannot carry serial. DFU trigger works on any
  cable, but the serial test needs SBU.
- A target Mac (Apple Silicon or T2) you can safely put into DFU.

## Wiring (bench pin map)

All pins are RP2040 GPIO numbers as printed on the Pico. See `src/main.rs` for
the authoritative map (the `main()` peripheral setup).

| Signal | Pico GPIO | Connect to |
|--------|-----------|------------|
| I2C0 SDA | GP16 | FUSB302 SDA |
| I2C0 SCL | GP17 | FUSB302 SCL |
| FUSB302 INT | GP20 | FUSB302 INT (active low) |
| Target VBUS enable | GP19 | 5 V load-switch enable for target VBUS (see note) |
| Status LED | GP25 | on-board LED (PD contract up) |
| 1.2 V shifter supply enable | GP14 | enable for the 1.2 V rail / translator Vcc(low) |
| SBU1 data | GP12 | 74AVC1T45 #1, A-side (RP2040) |
| SBU2 data | GP13 | 74AVC1T45 #2, A-side (RP2040) |
| SBU1 dir | GP10 | 74AVC1T45 #1 DIR |
| SBU2 dir | GP11 | 74AVC1T45 #2 DIR |

FUSB302 breakout to the **target** USB-C breakout:

- FUSB302 CC1 -> target CC1, FUSB302 CC2 -> target CC2.
- FUSB302 VBUS pin can be left unconnected (per the AltmodeFriend note) or tied
  to target VBUS for VBUS sensing; the firmware sources VBUS via the GP19 switch.
- Target D+/D- go to the host for DFU/restore. For M0 you can instead just watch
  the Mac re-enumerate in DFU on any host USB port; D+/D- passthrough is the
  real board's job (the USB 2.0 hub), not part of this bench proof.

74AVC1T45 wiring (per SBU line):

- **A-side** = RP2040 GPIO (GP12 or GP13), **Vcc(A)** = 3V3.
- **B-side** = the SBU pin on the target USB-C breakout, **Vcc(B)** = 1.2 V.
- **DIR** = GP10 / GP11. The firmware drives DIR **high** = A->B (RP2040 drives
  the target, i.e. our TX) and **low** = B->A (target drives us, i.e. our RX).
  If your translator's A/B sides are swapped relative to this, flip the
  `DIR_TO_TARGET` / `DIR_FROM_TARGET` constants in `src/main.rs`.

VBUS note: for a first light-up you can tie target VBUS to the Pico's 5 V (VBUS
pin) through a switch or even directly; the target only needs vSafe5V present for
PD signaling. GP19 drives the enable of a proper load switch on the real board.

## Build & flash

Shortcut: `just fw-build` / `just fw-flash` from the repo root do everything
below (flash includes the `bootsel` kick for a running dongle).

Prereqs (already handled if you set up the repo toolchain):

```
rustup target add thumbv6m-none-eabi
cargo install elf2uf2-rs flip-link
```

Build and make a UF2:

```
cargo build --release
elf2uf2-rs target/thumbv6m-none-eabi/release/dongle-lite-fw dongle-lite-fw.uf2
```

Flash: hold BOOTSEL on the Pico, plug it into the host, and copy
`dongle-lite-fw.uf2` onto the `RPI-RP2` drive. It reboots and enumerates as a
composite USB device with **two serial ports**:

- **CDC0** - control console.
- **CDC1** - the target's serial console (only carries data after `serial`).

With `probe-rs` and a debug probe you can instead `cargo run --release` and get
defmt logs.

### Updating over USB (no BOOTSEL button)

You only need the BOOTSEL button for the **first** flash of a build that has the
`bootsel` console command. After that, updates are button-free:

1. On **CDC0**, type `bootsel`. The firmware replies `ok bootsel; entering USB
   bootloader`, then reboots into the RP2040 bootrom via `reset_to_usb_boot` -
   the device drops off the bus and reappears as the `RPI-RP2` drive (and the
   picoboot interface).
2. Push the new image, any of:
   - `elf2uf2-rs -d target/thumbv6m-none-eabi/release/dongle-lite-fw` - builds
     nothing, just deploys the ELF to the mounted drive and the board reboots
     into it (`-d` = deploy).
   - `picotool load -x dongle-lite-fw.uf2` - loads and runs (`brew install
     picotool`).
   - drag `dongle-lite-fw.uf2` onto the `RPI-RP2` drive.

So the steady-state loop is: edit -> `cargo build --release` -> type `bootsel`
-> `elf2uf2-rs -d target/thumbv6m-none-eabi/release/dongle-lite-fw`. No hands on
the board.

Why the `bootsel` command is needed: while the firmware is running it presents
as a plain CDC device, so `picotool reboot -u` has no reset interface to grab.
The `bootsel` command *is* that reset path; once it drops to the bootrom,
picotool / elf2uf2 / drag-drop all work. (A true no-drive OTA - `embassy-boot` +
`embassy-usb-dfu`, updated with `dfu-util` - is possible later but needs a
flash-partitioned bootloader; overkill for the bench.)

## Bench test procedure

Open CDC0 in a terminal (`screen`, `picocom -b 115200`, etc.; baud on the
control port is irrelevant, it's USB CDC). You should see a greeting. Type
`help` for the command list. The console mirrors AsahiLinux `macvdmtool`:
`nop`, `dfu`, `reboot`, `serial`, `debugusb`, `reboot serial`,
`reboot debugusb`, plus `status`, `help`, and `bootsel` (firmware update).

Every command ends with a terminal status line so a host tool can drive it:
`ok <cmd>` on success, or `err <cmd> <reason>` where reason is `no-target` (no
target attached) or `no-ack` (the target didn't GoodCRC the VDM). Sends are
ack-confirmed - the `ok`/`err` reflects whether the port partner actually
acknowledged, not just that we transmitted.

### Test 1 - DFU trigger

1. Put the Mac in a normal on/booted state (DFU entry works from on; if the Mac
   is off, DFU entry timing differs - follow Apple's DFU key sequence for your
   model as a fallback).
2. Plug the target cable (Mac <- full USB-C cable <- USB-C breakout <- FUSB302).
3. On CDC0 you should see a connect line with the detected polarity, `VBUS on`,
   and the PD contract messages. LED (GP25) lights when the contract is up.
4. Type `dfu`. The firmware sends the Apple DFU-hold VDM
   (`5AC8012 0106 80010000`).
5. **Pass:** the Mac drops off and re-enumerates as **Apple Mobile Device (DFU
   Mode)** - check with `system_profiler SPUSBDataType` on a mac host, `lsusb`
   (05ac:1281) on Linux, or Apple Configurator / restorekit seeing a DFU device.

If nothing happens: check `status` shows `connected`/`idle` (contract up). If it
stays `disconnected`, the FUSB302 isn't seeing the target's Rd - recheck CC1/CC2
wiring and that VBUS is present.

### Test 2 - SBU serial, orientation A

1. With the target connected and a PD contract up, type `serial`. The firmware
   sends the UART-over-SBU mux VDM (`5AC8012 01840306`), powers the 1.2 V
   translators, sets the shifter directions for the detected polarity, and
   bridges the UART to **CDC1**.
2. Open **CDC1** at **115200 8N1**.
3. **Pass:** you see the target's AP/SEP console output (boot log / prompt).
   Typing into CDC1 reaches the target.

### Test 3 - SBU serial, orientation B (the important one)

1. Unplug the target cable, **flip it end-for-end**, and plug back in. (Reset the
   Pico - tap RUN or re-plug - so serial mode re-initialises for the new
   orientation; see the limitation below.)
2. Wait for the PD contract, then `serial` again, and reopen CDC1 at 115200.
3. **Pass:** console output is identical to Test 2. This proves the orientation
   swap (active-CC-side SBU -> our RX, other SBU -> our TX) and the 1.2 V level
   shift both work either way up. **This is the M0 go/no-go for layout.**

Report the results of all three (and note which cable worked - many "USB 3"
cables are actually USB 2 and won't carry SBU) before moving to M1 schematic.

## Host control over USB (recoverkit SDK)

Besides the human CDC console, the firmware exposes a **vendor-specific USB
interface** (`bInterfaceClass = 0xFF`) that the `restorekit` SDK drives over
`nusb` control transfers — no serial port, no OS driver, works the same on
macOS/Linux/Windows (Windows needs the interface bound to WinUSB). The device
also enumerates with a **unique USB serial** derived from the RP2040 flash UID
(e.g. `DPL-1A2B3C4D`), so multiple dongles are individually addressable.

Vendor control protocol (interface recipient, `wIndex` = the vendor interface
number):

- `VREQ_CMD` (`bRequest = 0x01`, OUT): `wValue` selects the command —
  `0` nop, `1` dfu, `2` reboot, `3` serial, `4` debugusb. Enqueues onto the same
  command path as the console and marks the result *pending*.
- `VREQ_STATUS` (`bRequest = 0x02`, IN): 5 bytes
  `[version, pd_state, flags, last_result, seq]`. `flags` bit0 = target
  attached, bit1 = CC2 (flipped) polarity. `last_result`: `1` pending, `2` ok,
  `3` no-target, `4` no-ack. Host sends a command then polls status until the
  result settles.

From the CLI:

```
restorekit dongle list                 # list dongles + what's cabled to each
restorekit dongle dfu                   # DFU the Mac on the sole dongle
restorekit dongle dfu --dongle DPL-1A2B3C4D
restorekit dongle dfu --ecid 0xC60A81…  # pick the dongle that Mac is behind
restorekit dongle reboot                # reboot the cabled Mac
restorekit dongle status                # PD state, target attached, orientation
```

`--ecid` resolves to a dongle by USB topology: a Mac in DFU enumerates as a
sibling of the dongle under the same hub, so the SDK matches them by shared bus +
parent port path.

## Known M0 limitations (by design)

- **Serial orientation latches at first `serial`.** The PIO UART pins are chosen
  when you first issue `serial`, so to test the other orientation you re-plug and
  reset the Pico. The production firmware re-inits the UART on every plug event;
  for a bench proof, reset-to-flip is fine and keeps the firmware simple.
- **No USB 2.0 hub / D+/D- passthrough.** M0 proves CC/VDM and SBU only. The hub
  that lets the host see the control MCU and the DFU target on one cable is the
  real board's net-new work (PRD section 6), not part of the bench proof.
- **VBUS is vSafe5V signaling only.** The dongle never powers or charges the
  target; the Mac restores on its own battery. That's the Lite scope.

## Layout of the firmware

- `src/fusb302.rs` - async FUSB302B register driver (source mode), ported from
  the Chromium OS / Reclaimer Labs / Asahi C driver.
- `src/main.rs` - USB composite device, the control console, the PD source state
  machine (`Engine`), the Apple VDMs, and the orientation-aware SBU bridge.
