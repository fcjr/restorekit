<div align="center">

# applerestore

**Reformat an Apple Silicon Mac with one command.**

Trigger DFU, detect the target, download the right firmware, and restore —
in a single self-contained binary.

[![CI](https://github.com/fcjr/applerestore/actions/workflows/ci.yml/badge.svg)](https://github.com/fcjr/applerestore/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/applerestore-cli.svg)](https://crates.io/crates/applerestore-cli)
[![docs.rs](https://img.shields.io/docsrs/applerestore)](https://docs.rs/applerestore)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

</div>

---

`applerestore` replaces the Apple Configurator dance with a single tool. It puts
a cabled Mac into DFU mode, identifies exactly which Mac it is, fetches the
matching macOS IPSW, and restores it — start to finish. It statically links the
[idevicerestore](https://github.com/libimobiledevice/idevicerestore) stack, so
there's nothing else to install: no Apple Configurator, no `brew install
idevicerestore`, no loose libraries.

> [!WARNING]
> A restore **erases everything** on the target Mac. Double-check you have the
> right device and a backup before running `restore` or `run`.

## Quickstart

```sh
brew install fcjr/fcjr/applerestore-cli
```

Cable the target Mac to your host's [DFU port](#dfu-port), then:

```sh
sudo applerestore run
```

That's the whole flow: it triggers DFU, waits for the device, downloads the
correct firmware (cached for next time), asks you to confirm the erase, and
restores. The target reboots into Setup Assistant.

## Commands

| Command | What it does |
| --- | --- |
| `applerestore status` | List Macs currently in DFU mode. |
| `applerestore dfu` | Put the cabled target into DFU mode. *(Apple Silicon macOS host, `sudo`.)* |
| `applerestore reboot` | Reboot the target back out of DFU. |
| `applerestore download` | Resolve and download firmware for the detected device. |
| `applerestore restore` | Erase-restore the detected device (confirms first). |
| `applerestore run` | The full flow: trigger → wait → download → restore. |
| `applerestore cache` | Show or clear the firmware cache. |

Handy flags: `--os-version 26.5.2` pins a build · `--ipsw ./file.ipsw` uses a
local firmware · `--revive` keeps user data instead of erasing · `--yes` skips
the confirmation · `--json` emits newline-delimited JSON events · `-v` streams
the full restore log.

## How it works

1. **Trigger** — On an Apple Silicon Mac host, `applerestore` sends Apple USB-PD
   Vendor Defined Messages through the host's Type-C port controller to reboot
   the target into DFU (a Rust port of
   [macvdmtool](https://github.com/AsahiLinux/macvdmtool)). On other hosts it
   prints the manual key-combo instead.
2. **Detect** — It scans USB for a Mac in DFU mode and reads the chip and board
   IDs from the device's serial string to identify the exact model.
3. **Fetch** — It resolves the correct IPSW from the [ipsw.me](https://ipsw.me)
   API (falling back to Apple's own feed), then downloads it — resumable and
   checksum-verified — into `${XDG_CONFIG_HOME:-~/.config}/applerestore/firmwares`.
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

Triggering DFU electronically needs an Apple Silicon Mac host and `sudo`;
everywhere else, put the target into DFU by hand and `applerestore` takes it from
there. On Linux the restore phase talks to the device through `usbmuxd` — make
sure it's installed and running.

## As a library

The CLI is a thin shell over the [`applerestore`](https://docs.rs/applerestore)
crate, which exposes the whole workflow with no I/O of its own — every operation
reports progress through a callback, so you can build a GUI on the same engine.

```rust
use applerestore::{dfu, firmware};
use std::time::Duration;

let device = dfu::wait_for_dfu(Duration::from_secs(60))?;
let fw = firmware::resolve(device.identifier().unwrap(), None)?;
let cache = firmware::default_cache_dir()?;
let ipsw = firmware::download(&cache, &fw, &mut |event| {
    // render progress however you like
})?;
```

See the [API docs](https://docs.rs/applerestore) for the full surface.

## Building from source

```sh
git clone --recurse-submodules https://github.com/fcjr/applerestore
cd applerestore
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

## Releasing

Releases are automated: bump the version, push a `v*` tag, and CI publishes the
GitHub Release, the Homebrew cask, and the crates. See
[docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

## License

Apache-2.0 — see [LICENSE](LICENSE) and [NOTICE](NOTICE). The DFU-trigger code is
a Rust port of macvdmtool (also Apache-2.0); the vendored C libraries keep their
own licenses.
