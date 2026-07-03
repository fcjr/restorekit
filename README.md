# applerestore

Restore Apple Silicon Macs over USB — from the command line, in one tool.

`applerestore` triggers DFU mode, detects the target Mac, downloads the correct
macOS firmware, and restores it. It statically links the
[idevicerestore](https://github.com/libimobiledevice/idevicerestore) stack, so
the binary is **self-contained**: no `brew install idevicerestore`, no external
libraries to manage.

> [!WARNING]
> A restore **erases all data** on the target Mac. Make sure you have the right
> device and backups before running `restore` or `run`.

## Install

```sh
brew install fcjr/fcjr/applerestore
```

Or grab a binary from the [releases page](https://github.com/fcjr/applerestore/releases).

## Usage

```sh
# List Macs currently in DFU mode
applerestore status

# Put a cabled target into DFU mode (Apple Silicon macOS host, needs sudo)
sudo applerestore dfu

# Download the correct firmware for the detected DFU device
applerestore download

# Erase-restore the detected device (prompts before erasing)
applerestore restore

# One command: trigger DFU, wait, download, restore
sudo applerestore run

# Manage the firmware cache
applerestore cache
applerestore cache --clear
```

Useful flags: `--os-version 26.5.2` to pin a build, `--ipsw ./file.ipsw` to use a
local firmware, `--revive` for an update-style restore that keeps data, `--yes`
to skip the erase confirmation, `--json` for machine-readable event output.

## How it works

1. **DFU trigger** (macOS on Apple Silicon only) — sends Apple USB-PD Vendor
   Defined Messages through the host's Type-C port controller to reboot the
   target into DFU, the technique from
   [macvdmtool](https://github.com/AsahiLinux/macvdmtool). On other hosts,
   `applerestore` prints manual DFU-entry instructions instead.
2. **Detection** — enumerates USB for a Mac in DFU mode and identifies its exact
   model from the chip/board IDs in the USB serial string.
3. **Firmware** — resolves the right IPSW via the [ipsw.me](https://ipsw.me) API
   (with Apple's official feed as a fallback), downloads it resumably, verifies
   its checksum, and caches it under
   `${XDG_CONFIG_HOME:-~/.config}/applerestore/firmwares`.
4. **Restore** — drives the statically-linked `libidevicerestore` to restore or
   revive the device.

## DFU port

Connect the host and target with a USB-C cable using the target's **DFU port**:

| Target | DFU port |
| --- | --- |
| MacBook Air / 13" Pro | Left side, port nearest the screen |
| 14" / 16" MacBook Pro | Left side, port next to MagSafe |
| Mac mini / Studio | Port nearest the power button (front, for Studio) |
| iMac | Port nearest the edge |

Triggering DFU electronically requires an **Apple Silicon Mac host** and `sudo`.
Detection, download, and restore work on macOS and Linux.

### Linux notes

The restore phase talks to the device through `usbmuxd`; make sure it's
installed and running. Building from source needs `autoconf`, `automake`,
`libtool`, `pkg-config`, `cmake`, and `libusb-1.0` development headers.

## Building from source

```sh
git clone --recurse-submodules https://github.com/fcjr/applerestore
cd applerestore
cargo build --release
```

The build compiles the full idevicerestore C stack from pinned submodules; the
first build takes a few minutes.

## License

MIT — see [LICENSE](LICENSE). The DFU-trigger implementation is a Rust port of
macvdmtool and retains its Apache-2.0 licensing; the vendored C libraries keep
their respective licenses. See [NOTICE](NOTICE).
