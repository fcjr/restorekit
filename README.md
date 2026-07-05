# restorekit

**Reformat an Apple Silicon mac from any platform in one command.**

[![CI](https://github.com/fcjr/restorekit/actions/workflows/ci.yml/badge.svg)](https://github.com/fcjr/restorekit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/restorekit-cli.svg)](https://crates.io/crates/restorekit-cli)
[![docs.rs](https://img.shields.io/docsrs/restorekit)](https://docs.rs/restorekit)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

</div>

---

`restorekit` is a standalone rust library, cli tool, and tauri gui that lets you fully wipe or restore a T2 or M series macbook without using any apple tools.  It works on a completely cross platform* stack thanks to rust and [libirecovery](https://github.com/libimobiledevice/libirecovery) and does not require any additional software.

* Automatic DFU is currently only supported on macOS due to hardware limitations.

## Why?

I've worked at a few places where windows was the default machine (including for IT) and macs were only issued when requested or required.  A lot of times the IT folks are forced to use a macbook simply because they need one to recover and reset their fleet of macbooks.  I've also seen companies forced to ship a whole new mac out to employees in locations without an apple store when a simple reset would have solved the problem.  I think this sucks.  People should be able to repair their mac without needing to own another one!

## Install the CLI

```sh
brew install fcjr/fcjr/restorekit-cli   # Homebrew (macOS + Linux)
# or on any platform
cargo install restorekit-cli            # from crates.io
# or on windows
scoop bucket add fcjr https://github.com/fcjr/scoop-fcjr
scoop install restorekit-cli
```

Or grab a binary from the [releases](https://github.com/fcjr/restorekit/releases).

## Prefer a GUI?

There is a very WIP tauri **[desktop app](#desktop-app)** that wraps the same library.

## Quickstart

Plug the target mac into your hosts [DFU port](#dfu-port).  If you are on on mac, restorekit will automatically set the target machine into the DFU mode.  If you are on linux or windows, follow [this guide](#dfu-port) to get the laptop ready for restore.

If you are on windows, next run `restorekit setup-driver` to install our custom winusb driver (you can skip this on other platforms.)

Then run:

```sh
sudo restorekit run
```

Follow the instructions and bam! restorekit will detect the mac, download the approprate firmware, and restore the machine to factory settings.

On linux, you can avoid `sudo` by installing a [udev rule](#linux-usb-permissions) first.

## More commands

restorekit supports various other comamnds such depending on the platform it is running on.

Type `restorekit -h` to learn more.

The cli plays nice with automation by exposing a `--json` flag on most commands.

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

Triggering DFU mode automatically requires the host machine to be a T2 or M series mac. Other devices lack the hardware to do so automatically so you need to set the mac into DFU mode manually.

## Linux USB permissions

On Linux, `restorekit` needs write access to apple usb devices, this can be forced by running it as sudo, but if you'd like to avoid sudo you can install a udev rule.

Copy [./udev/51-restorekit.rules] to `/etc/udev/rules.d/`, then run:

```sh
sudo udevadm control --reload-rules && sudo udevadm trigger
```

After installing the udev rule, you may have to unplug and replug your device to get the permmissions to apply.

The `.deb` package _should_ this rule automatically.

## Desktop app

There is also a WIP tauri + svelte gui that wraps the `restorekit` library for an easier one-click
restore.  It has the same functionality and can currently be installd on macOS via homebrew. 

For linux (.deb, .AppImage) or windows, you can download the gui directly from the
[releases page](https://github.com/fcjr/restorekit/releases). Once installed it has full
automatic updates via tauri's auto-updater.

```sh
brew install --cask fcjr/fcjr/restorekit   # macOS
```

Winget support coming when I figure out how to automate the process better.

## As a library

Both the CLI and the desktop app are thin shells over the [`restorekit`](https://docs.rs/restorekit)
rust crate, which exposes the same workflow using a callback based system:

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

See the [api docs](https://docs.rs/restorekit) more details.

## License

Apache-2.0: see [LICENSE](LICENSE) and [NOTICE](NOTICE). The DFU code code is
a rust port of [macvdmtool](https://github.com/AsahiLinux/macvdmtool) (also Apache-2.0);
the vendored C libraries keep their own licenses.
