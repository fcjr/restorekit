# restorekit

**Reformat any Apple Silicon (M series) mac from macOS, linux or windows with a single command.**

[![CI](https://github.com/fcjr/restorekit/actions/workflows/ci.yml/badge.svg)](https://github.com/fcjr/restorekit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/restorekit-cli.svg)](https://crates.io/crates/restorekit-cli)
[![docs.rs](https://img.shields.io/docsrs/restorekit)](https://docs.rs/restorekit)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

</div>

---

`restorekit` is a standalone rust library, cli tool, and tauri gui that lets you fully wipe or restore a T2 or M series macbook without using any apple tools.  It works on a completely cross platform* stack thanks to rust and [libirecovery](https://github.com/libimobiledevice/libirecovery) and does not require any additional software or configuration.

\* Automatic DFU is currently only supported on macOS due to hardware limitations.

## Why?

I've worked at a few places where windows was the default machine (including for IT) and macs were only issued when requested or required.  A lot of times the IT folks are forced to use a macbook simply because they need one to recover and reset their fleet of macbooks.  I've also seen companies forced to ship a whole new mac out to employees in locations without an apple store when a simple reset would have solved the problem.  I think this sucks.  People should be able to repair their mac without needing to own another one!

## Install the CLI

```sh
# on macOS or linux, via homebrew
brew install fcjr/fcjr/restorekit-cli
# or on windows via scoop
scoop bucket add fcjr https://github.com/fcjr/scoop-fcjr
scoop install restorekit-cli
```

Or grab a binary from the [releases](https://github.com/fcjr/restorekit/releases).

Building from source (`cargo install` or `cargo build`) compiles a vendored c stack, so it needs autotools, cmake and a c compiler (even more stuf like MSYS2 on windows).  All binaries above are statically linked so
do not require any prerequisites.
If you still prefer buliding from source, first setup your env via the [build guide](docs/building.md).
Then you can insatll via cargo:

```sh
# (needs a c toolchain; see above)
cargo install restorekit-cli
```

## Install the Desktop app

There is also a WIP tauri + svelte gui that wraps the `restorekit` library for an easier one-click
restore.  It has the same functionality and can currently be installd on macOS via homebrew.

```sh
brew install --cask fcjr/fcjr/restorekit   # macOS
```

For linux (.deb, .AppImage) or windows, you can download the gui directly from the
[releases page](https://github.com/fcjr/restorekit/releases). Once installed it has full
automatic updates via tauri's auto-updater.

Winget support coming when I figure out how to automate the process better.

## Quickstart

Plug the target mac into your hosts [DFU port](#dfu-port).  If you are on on mac, restorekit will automatically set the target machine into the DFU mode.  If you are on linux or windows, follow [this guide](#dfu-port) to get the laptop ready for restore.

If you are on windows, next run `restorekit setup-driver` to install our custom winusb driver (you can skip this on other platforms.)

Then run:

```sh
sudo restorekit restore
# or by eid
sudo restorekit restore --ecid 0xc60a812345678 
# or with no prompts
sudo restorekit restore --yes
```

Follow the instructions and bam! restorekit will detect the mac, download the approprate firmware, and restore the machine to factory settings.

On linux, you can avoid `sudo` by installing a [udev rule](#linux-usb-permissions) first.

## More commands

restorekit supports various other comamnds such depending on the platform it is running on.

Type `restorekit -h` to learn more.

The cli plays nice with automation by exposing a `--json` flag on most commands.

## DFU port

Unfortunately apple wasn't consistent when choosing what usb port to make the DFU (device firmware upgrade) port...  Below is a list of where you can find it on some of the most common macs. For a full list see
Apple's [official documentation](https://support.apple.com/en-us/120694).

| Target | DFU port |
| --- | --- |
| 14" / 16" MacBook Pro | Left side, port next to macsafe |
| Mac mini / Studio | Port closest to the power button |
| MacBook Air / 13" Pro | Left side, port closest to the hinge |
| iMac | Port closest to the edge |

If you are recovering from a linux or windows machine you must enter DFU mode on the target device manually.
The unofficial [apple wiki](https://theapplewiki.com/wiki/DFU_Mode#Mac_with_Apple_Silicon) has a great guide for every machine [here](https://theapplewiki.com/wiki/DFU_Mode#Mac_with_Apple_Silicon).

## Platform support

All features except for automatic DFU mode are available on all platform. Entering DFU mode automatically requires the host machine to be a T2 or M series mac. Other devices lack the hardware to do so automatically so you need to set the mac into DFU mode manually.

## Linux USB permissions

On Linux, `restorekit` needs write access to apple usb devices, this can be forced by running it as sudo, but if you'd like to avoid sudo you can install a udev rule.

Copy [./udev/51-restorekit.rules] to `/etc/udev/rules.d/`, then run:

```sh
sudo udevadm control --reload-rules && sudo udevadm trigger
```

After installing the udev rule, you may have to unplug and replug your device to get the permmissions to apply.

The `.deb` package _should_ this rule automatically.

## As a library

Both the CLI and the desktop app are thin shells over the [`restorekit`](https://docs.rs/restorekit)
rust crate, which exposes the same workflow using a callback based system:

```rust
use restorekit::{device, firmware};
use std::time::Duration;

// The sole Mac in DFU mode. device::list() shows everything connected (in any
// mode), Target::Ecid(..) picks one of several, and dev.enter_dfu(..) puts a
// cabled target into DFU on hosts that support it.
let dev = device::wait(device::Target::One, Duration::from_secs(60))?;
let fw = firmware::resolve(dev.identifier().unwrap(), None)?;
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
