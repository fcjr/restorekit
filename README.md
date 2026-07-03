<div align="center">

# restorekit

**Reformat an Apple Silicon Mac with one command.**

Trigger DFU, detect the target, download the right firmware, and restore.
A cross-platform CLI and desktop app for macOS, Linux, and Windows.

[![CI](https://github.com/fcjr/restorekit/actions/workflows/ci.yml/badge.svg)](https://github.com/fcjr/restorekit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/restorekit-cli.svg)](https://crates.io/crates/restorekit-cli)
[![docs.rs](https://img.shields.io/docsrs/restorekit)](https://docs.rs/restorekit)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

</div>

---

`restorekit` replaces the Apple Configurator dance with a single tool. It puts
a cabled Mac into DFU mode, identifies exactly which Mac it is, fetches the
matching macOS IPSW, and restores it — start to finish. It runs on **macOS,
Linux, and Windows**, as a command-line tool or a **[desktop app](#desktop-app)**
built on the same engine.

> [!WARNING]
> A restore **erases everything** on the target Mac. Double-check you have the
> right device and a backup before running `restore` or `run`.

## Install

```sh
brew install fcjr/fcjr/restorekit-cli   # Homebrew (macOS + Linux)
cargo install restorekit-cli            # from crates.io
```

Or grab a binary from the [releases page](https://github.com/fcjr/restorekit/releases).
Prefer a GUI? The **[desktop app](#desktop-app)** wraps the same engine.

## Quickstart

Cable the target Mac to your host's [DFU port](#dfu-port), then:

```sh
sudo restorekit run
```

That's the whole flow: it triggers DFU, waits for the device, downloads the
correct firmware (cached for next time), asks you to confirm the erase, and
restores. The target reboots into Setup Assistant.

On Linux, you can avoid `sudo` by installing a [udev
rule](#linux-usb-permissions) first.

## Commands

| Command | What it does |
| --- | --- |
| `restorekit status` | List Macs currently in DFU mode. |
| `restorekit dfu` | Put the cabled target into DFU mode. *(Apple Silicon macOS host, `sudo`.)* |
| `restorekit reboot` | Reboot the target back out of DFU. |
| `restorekit download` | Resolve and download firmware for the detected device. |
| `restorekit restore` | Erase-restore the detected device (confirms first). |
| `restorekit run` | The full flow: trigger → wait → download → restore. |
| `restorekit cache` | Show or clear the firmware cache. |
| `restorekit setup-driver` | Bind the WinUSB driver to the cabled Mac. *(Windows, one-time; elevates.)* |

Handy flags: `--os-version 26.5.2` pins a build · `--ipsw ./file.ipsw` uses a
local firmware · `--revive` keeps user data instead of erasing · `--yes` skips
the confirmation · `--json` emits newline-delimited JSON events · `-v` streams
the full restore log.

## How it works

1. **Trigger** — On an Apple Silicon Mac host, `restorekit` sends Apple USB-PD
   Vendor Defined Messages through the host's Type-C port controller to reboot
   the target into DFU (a Rust port of
   [macvdmtool](https://github.com/AsahiLinux/macvdmtool)). On other hosts it
   prints the manual key-combo instead.
2. **Detect** — It scans USB for a Mac in DFU mode and reads the chip and board
   IDs from the device's serial string to identify the exact model.
3. **Fetch** — It resolves the correct IPSW from the [ipsw.me](https://ipsw.me)
   API (falling back to Apple's own feed), then downloads it — resumable and
   checksum-verified — into `${XDG_CONFIG_HOME:-~/.config}/restorekit/firmwares`.
4. **Restore** — It drives the statically-linked `idevicerestore` to restore or
   revive the device, reporting each step.

## DFU port

Use a data-capable USB-C (or Thunderbolt) cable and the target's **DFU port**:

| Target | DFU port |
| --- | --- |
| MacBook Air / 13" Pro | Left side, port nearest the screen |
| 14" / 16" MacBook Pro | Left side, port next to MagSafe |
| Mac mini / Studio | Port nearest the power button |
| iMac | Port nearest the edge |

## Platform support

| | Trigger DFU | Detect | Download | Restore |
| --- | :---: | :---: | :---: | :---: |
| **macOS** (Apple Silicon) | ✅ | ✅ | ✅ | ✅ |
| **macOS** (Intel) | — | ✅ | ✅ | ✅ |
| **Linux** | — | ✅ | ✅ | ✅ |
| **Windows** | — | ✅ | ✅ | ✅ |

Triggering DFU electronically needs an Apple Silicon Mac host and `sudo`;
everywhere else, put the target into DFU by hand and `restorekit` takes it from
there. On Linux, `restorekit` embeds its own usbmuxd server — no external daemon
needed — but requires USB device access (see [Linux USB
permissions](#linux-usb-permissions) below). On Windows the target in DFU/recovery
needs the **WinUSB** driver bound to it: run **`restorekit setup-driver`** once
(the desktop app offers a one-click *Set up USB access*), and it's installed for
every Mac after that. It's a self-signed driver package, so approve the one-time
Windows prompt. (Prefer to do it by hand? [Zadig](https://zadig.akeo.ie/) or the
[UsbDk](https://github.com/daynix/UsbDk) filter work too.)

## Linux USB permissions

On Linux, `restorekit` needs write access to Apple USB devices. You can either
run with `sudo`, or install a udev rule so it works without root:

```sh
sudo cp udev/51-restorekit.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

After installing the rule, **unplug and re-plug** the device (or re-enter DFU)
so the new permissions apply. This is a one-time setup — the rule persists
across reboots.

The `.deb` package installs this rule automatically. For the `.AppImage` or a
`cargo install` build, copy it manually as shown above.

## Desktop app

A cross-platform app (Tauri + Svelte) wraps the same engine for a point-and-click
restore: detect the device, download, confirm, restore — with live progress. It
links the `restorekit` library directly, so it's the same code path as the CLI.

```sh
brew install --cask restorekit          # macOS
```

On Linux, grab the `.deb` or `.AppImage`, and on Windows the installer, from the
[releases page](https://github.com/fcjr/restorekit/releases). One-click *Enter
DFU* needs an Apple Silicon Mac host; on other hosts the app shows the manual
key-combo and takes over once the target is in DFU.

See [docs/gui-prd.md](docs/gui-prd.md) for the app's design.

## As a library

The CLI is a thin shell over the [`restorekit`](https://docs.rs/restorekit)
crate, which exposes the whole workflow with no I/O of its own — every operation
reports progress through a callback, so you can build a GUI on the same engine.

```rust
use restorekit::{dfu, firmware};
use std::time::Duration;

let device = dfu::wait_for_dfu(Duration::from_secs(60))?;
let fw = firmware::resolve(device.identifier().unwrap(), None)?;
let cache = firmware::default_cache_dir()?;
let ipsw = firmware::download(&cache, &fw, &mut |event| {
    // render progress however you like
})?;
```

See the [API docs](https://docs.rs/restorekit) for the full surface.

## Building from source

```sh
git clone --recurse-submodules https://github.com/fcjr/restorekit
cd restorekit
cargo build --release
```

The build compiles the full idevicerestore C stack from pinned submodules, so
the first build takes a few minutes. On Linux, install the toolchain first:

```sh
sudo apt-get install -y \
  build-essential autoconf automake libtool pkg-config cmake autoconf-archive \
  libusb-1.0-0-dev libssl-dev libcurl4-openssl-dev zlib1g-dev
```

Those `-dev` packages only satisfy the vendored libraries' `configure` checks —
OpenSSL, libcurl, and zlib are still linked statically, so the finished binary
depends only on `libc` and `libusb`.

### Windows

The C stack is built with autotools, which needs the GNU toolchain, so Windows
builds target `x86_64-pc-windows-gnu` from an [MSYS2](https://www.msys2.org/)
**MINGW64** shell. In that shell:

```sh
pacman -S --needed base-devel git make autoconf automake libtool \
  autoconf-archive gettext-devel pkgconf perl \
  mingw-w64-x86_64-gcc mingw-w64-x86_64-pkgconf mingw-w64-x86_64-cmake \
  mingw-w64-x86_64-libusb mingw-w64-x86_64-nasm
rustup default stable-x86_64-pc-windows-gnu   # GNU host toolchain
rustup target add x86_64-pc-windows-gnu
```

Then, with the Windows `cargo` on `PATH`, build from the repo root:

```sh
cargo build --release -p restorekit-cli --target x86_64-pc-windows-gnu
```

The `-gnu` host toolchain matters: build scripts link with `ld` instead of
MSVC's `link.exe`, which MSYS2's coreutils `link` would otherwise shadow. As on
Linux, OpenSSL/libcurl/zlib link statically; the binary needs only `libusb`.

## Releasing

Releases are automated: bump the version, push a `v*` tag, and CI publishes the
GitHub Release, the Homebrew cask, and the crates. See
[docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

## License

Apache-2.0 — see [LICENSE](LICENSE) and [NOTICE](NOTICE). The DFU-trigger code is
a Rust port of macvdmtool (also Apache-2.0); the vendored C libraries keep their
own licenses.
