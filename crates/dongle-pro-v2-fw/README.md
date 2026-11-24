# dongle-pro-v2-fw

Firmware for the Dongle-Pro v2 and the Dongle-Pro v2 PD, two binaries:

- `dongle-pro-v2` (`src/bin/`): the product firmware for the v2, what
  restorekit talks to. RecoverKit's 16D0:14F0 as `Dongle-Pro-v2`, serial
  `DP2-` plus 8 hex digits of the chip UID. It answers the vendor interface in
  `restorekit-dongle-proto` (dfu, reboot, debugusb, serial, status, version,
  bootsel), logs to its first CDC port and bridges the target's SBU UART to its
  second after `serial`, again after every new PD contract. Not for the PD
  board yet.
- `dongle-pro-v2-fw` (`src/main.rs`): bench firmware for both boards. A USB
  serial console that drives every part of the board by hand, for each board's
  "check before ordering" list.

Both are built on [ch32-hal](https://github.com/ch32-rs/ch32-hal) and
embassy-usb. On the first v2 board: PD contract, `dfu`, `reboot`, firmware
updates and USB 3 passthrough (an iPhone at 10 Gb/s) work. `serial` gets the
Mac to switch its UART onto SBU, but I haven't seen a byte from a production
Mac yet. The PD build has not run on a board.

## Building

```sh
cd crates/dongle-pro-v2-fw
cargo build --release --bin dongle-pro-v2                   # product, v2
cargo build --release --bin dongle-pro-v2-fw                # bench, v2
cargo build --release --bin dongle-pro-v2-fw --features pd  # bench, v2 PD
rust-objcopy -O binary target/riscv32imc-unknown-none-elf/release/dongle-pro-v2 dongle-pro-v2-fw.bin
```

Needs `rustup target add riscv32imc-unknown-none-elf` and, for
`rust-objcopy`, `cargo install cargo-binutils` plus the `llvm-tools`
component.

## Flashing

On a board running the product firmware:
`restorekit dongle update --file dongle-pro-v2-fw.bin`. The 62 KB of flash has
no room for a second image, so restorekit sends `bootsel`, writes the chip
through its ROM USB loader and verifies it. Without `--file` it installs the
latest `dongle-pro-v2-v*` release.

Anything else, a blank board or one running the bench firmware: get it into the
ROM loader (`1a86:55e0`) and use [wchisp](https://github.com/ch32-rs/wchisp):
`wchisp flash dongle-pro-v2-fw.bin`. Both firmwares enter the loader when their
first CDC port is opened at 1200 baud and closed again. A blank board needs the
`boot` button held while it's plugged into the host port. The MCU sits behind
the CH334P, so just seeing the console come up answers the first question: the
hub and the MCU's USB work.

The bench console uses the pid.codes test ID (1209:0001) on purpose, so restorekit
does not mistake it for a dongle.

Size, of 62 KB: product 41 KB; bench 53 KB for the v2, 59 KB for the PD board.

## Releasing

Run `bump-release-fw` from the Actions tab with `pro-v2`, or
`scripts/bump-fw-version.sh patch pro-v2` and push the tag it prints. The
`dongle-pro-v2-v*` tag builds the image and attaches it to a release, which is
where `restorekit dongle update` finds it.

## What it does by itself

- Finds the HUSB311 and puts Rp on both CC lines.
- When a target shows Rd: picks the cable orientation, turns on 5 V, sends an
  empty Apple VDM on SOP'' Debug and a Source_Capabilities, and answers the
  Request. Every PD message in either direction is printed. It keeps sending
  Source_Capabilities until the target takes a contract, then stops, so an
  established contract isn't renegotiated. Get_Source_Cap and Soft_Reset start
  it over.
- If the target answers `dfu` with a hard reset within 5 s (a Mac that has
  just booted can), `dfu` is sent again after 5 s, up to three times. Same as
  the Lite.
- On the PD board: notices a charger, and drops the CH224Q from its 9 V
  default to 5 V straight away. It never passes charger power to the target
  unless told to.
- Turns target power off when the target goes away.

`auto off` stops all of that if you want to do it by hand.

## Commands

`help` prints them. Numbers are hex unless it says otherwise.

| Command | What it is for |
|---|---|
| `status` | state, HUSB311 registers, power pins, charger |
| `scan` | I2C addresses that answer on the HUSB311 bus |
| `tr REG [N]`, `tw REG VAL` | read and write HUSB311 registers |
| `rp 0\|1\|2`, `open` | Rp level, or no Rp |
| `caps lite\|5v` | advertise the Lite's PDO (fixed 5 V, 450 mA, USB comms) or 5 V at the host limit |
| `poke` | empty Apple VDM on SOP'' Debug |
| `dfu`, `reboot`, `debugusb` | the Apple VDMs, same as the Lite |
| `replug` | target power off and CC open for a second, then attach again |
| `vdm W0 W1 ..` | any VDM on SOP'' Debug |
| `hardreset` | PD hard reset |
| `serial` | Apple serial VDM, then bridge the SBU UART to the console (Ctrl-X leaves) |
| `bridge 1\|2`, `sbu on\|off` | the bridge and the level shifters without the VDM |
| `vbus on\|off` | v2: the target VBUS switch |
| `host`, `chg`, `hv` `on\|off` | PD: the three power mux controls |
| `chgs`, `chgv V`, `cr`, `cw` | PD: charger status, ask for a voltage, CH224Q registers |
| `caps chg` | PD: offer the target the charger's PDOs and follow its Request |

## Which check each one answers

v2 list (`hardware/dongle-pro-v2/README.md`):

- 2, SOP'' Debug: `poke` reports whether the Mac sent a GoodCRC. `dfu` reports
  how many of its VDMs were acknowledged, and replies on SOP'' Debug are
  printed, which the Lite cannot see.
- 3, the target mux with the HUSB311 driving CC: attach in both orientations
  and watch the SuperSpeed link.
- 4, CH334P self-powered: `vbus on` with a target in DFU and see if it
  enumerates on the host.
- 5, the FET level shifter: `sbu on`, `bridge 1` or `bridge 2`, scope the lines.
- 6, firmware fit: the sizes above.

PD list (`hardware/dongle-pro-v2-pd/README.md`):

- 1, the Mac takes a contract above 5 V: plug a charger in, `caps chg`, read
  the Request, measure `vbus`.
- 2, CH224Q in the 9 V strap mode: `chgs` should show its address and the
  charger's PDOs.
- 3 and 10, mux heat and both current limits: `host on` or `chg on` into an
  electronic load.
- 9, the SBU clamp FETs: same as the v2 serial check.
- 11, the 6.4 V trip: `chg on`, then `chgv 9`. Target VBUS should drop out.
  `hv on` should bring it back.

## What I am least sure of

- The HUSB311 driver is plain TCPCI plus the RT1711H soft reset. Any
  vendor-specific setup it turns out to need is not there. `tr` and `tw` are
  for finding out.
- The CH224Q register map (status 0x09, voltage 0x0A, capabilities at 0x60) is
  from its datasheet and untested.
- The bit-banged I2C needs the MCU's pull-ups on as well: the first v2
  board's bus reads all zeros without them. Don't slow `pause()` in
  `src/i2c.rs` down, or the Accept misses tSenderResponse.
- Which SBU line the target transmits on comes from the Lite: SBU1 with a
  normal cable (USART3 RX), SBU2 flipped (USART2 RX). Not yet confirmed with a
  Mac that actually prints.
