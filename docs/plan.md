# restorekit — Implementation Plan

Working checklist; check items off as they land.

## 1. Scaffold
- [x] Workspace `Cargo.toml` (crates/restorekit, crates/restorekit-cli), Apache-2.0 LICENSE + NOTICE (attribution for the vdm port and vendored libs), .gitignore
- [x] docs/prd.md
- [x] docs/plan.md (this file)
- [x] README.md (install, usage, DFU port locations, safety warning)

## 2. Library core
- [x] `error.rs` — thiserror `Error` enum covering discovery, resolution, download, checksum, restore, host-support failures
- [x] `progress.rs` — serializable `Event` enum; all long operations take `&mut dyn FnMut(Event)`

## 3. DFU discovery (cross-platform)
- [x] `dfu/discovery.rs` — nusb enumeration, VID 0x05ac / PID 0x1227, DFU serial-string parser (`CPID:... BDID:... ECID:...`)
- [x] `wait_for_dfu(timeout)` polling loop for post-trigger / manual DFU entry
- [x] `device.rs` — static (CPID, BDID) → (board, identifier, marketing name) table generated from ipsw.me `/v4/devices`; runtime ipsw.me fallback for unknown boards
- [x] Unit tests: serial parser, table lookup

## 4. Firmware resolve + cache + download
- [x] ipsw.me `/v4/device/{identifier}?type=ipsw` resolver (latest signed, or pinned `--os-version`)
- [x] mesu.apple.com `com_apple_macOSIPSW.xml` fallback resolver (latest only)
- [x] Cache dir resolution: `${XDG_CONFIG_HOME:-~/.config}/restorekit/firmwares` (+ env/flag overrides)
- [x] Resumable download (`Range` into `.partial`, atomic rename), SHA-256/SHA-1 verification, cache hit short-circuit
- [x] Unit tests: resolver parsing (JSON + plist fixtures), cache-dir resolution

## 5. DFU trigger (macOS Apple Silicon only)
- [x] `dfu/vdm.rs` — Rust port of macvdmtool: AppleHPM IOKit plug-in FFI (COM vtable), LOCK unlock w/ platform-name key, Gaid reset retry, DBMa enter/exit (RAII), VDMs send + reg 0x4d ack polling
- [x] `enter_dfu()` (VDM `{0x5ac8012, 0x106, 0x80010000}`) and `reboot()` (VDM `{0x5ac8012, 0x105, 0x80000000}`)
- [x] Root + Apple Silicon host guards with clear errors; manual DFU instructions helper for unsupported hosts
- [x] **Hardware-verified**: an Apple Silicon host triggered DFU on a target Mac, which was then detected and identified correctly

## 6. Restore engine (statically-linked libidevicerestore, FFI-only)
Self-contained binary: the idevicerestore C stack is built from pinned sources
and linked in. No subprocess, no `brew install idevicerestore`.
- [x] `restorekit-sys` crate: git submodules (`vendor/`) pinning libplist, libimobiledevice-glue, libusbmuxd, libirecovery, libtatsu, libimobiledevice, idevicerestore
- [x] `build.rs` builds the stack in cargo flow: openssl/zlib/curl via vendored `-sys` crates, libzip via CMake, the 6 autotools libs into a staging prefix, then compiles idevicerestore's `.c` sources (with `main` renamed) and emits static link directives + macOS frameworks
- [x] FFI decls (`idevicerestore_client_new/set_flags/set_ipsw/set_ecid/set_progress_callback/start/get_error`, `FLAG_ERASE`)
- [x] `restore.rs` rewritten over the FFI (progress callback → `Event::RestoreStep`), subprocess path removed
- [x] Native macOS build links cleanly (self-contained: no Homebrew dylib deps — verify with `otool -L`)
- [x] Unit tests: progress step-name mapping

## 7. CLI
- [x] clap derive skeleton: `status`, `dfu`, `reboot`, `download`, `restore`, `run`, `cache`; global `--cache-dir`, `--json`, `-v`
- [x] indicatif progress rendering from library events; JSON-lines mode
- [x] Erase confirmation prompt (model + ECID, `--yes` to skip)

## 8. CI/CD (native per-platform builds — the static C stack rules out zig cross-compile)
> Linux build verified locally in an ubuntu:24.04 Docker container (aarch64):
> full C stack builds, GNU ld links cleanly (no link-order tweaks needed),
> binary is self-contained (ldd: only libc + libusb). apt needs
> build-essential, autoconf-archive, libssl-dev, libcurl4-openssl-dev,
> zlib1g-dev beyond the obvious autotools/cmake/libusb.
- [x] `.github/workflows/ci.yml` — fmt, clippy, test; matrix macos-14 (arm64) + ubuntu (needs `libusb-1.0`, autotools, cmake to build the stack)
- [x] Native release build matrix: macos-14 (arm64), macos-13 (x86_64), ubuntu-24 arm64 + x86_64 — each runs `cargo build --release` (build.rs builds the self-contained stack), uploads the artifact
- [x] `.goreleaser.yaml` — `builder: prebuilt` assembling the per-platform artifacts into archives + checksums; `homebrew_casks` → fcjr/homebrew-fcjr with quarantine-removal hook
- [x] `.github/workflows/release.yml` — tag-triggered: build matrix → goreleaser packages prebuilt binaries; `GITHUB_TOKEN` + `TAP_GITHUB_TOKEN`

## 9. Verification
- [x] `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`
- [x] `restorekit status` / `download` smoke tests against live APIs
- [x] **Full firmware pipeline hardware-verified**: auto-detected the DFU device, resolved macOS 26.5.2, downloaded 19.8 GB (survived a mid-transfer drop via retry+resume), SHA-256 verified, cached with sidecar
- [ ] `goreleaser check` (goreleaser not installed locally — validated in CI)
- [x] Release build (`cargo build --release`) links; 8.2 MB self-contained binary, no third-party dylibs
- [x] Hardware: `sudo restorekit dfu` + `status` verified against the cabled target Mac (detection + model ID confirmed)
- [x] **Full restore hardware-verified**: end-to-end erase-restore of an M1 Pro over the FFI succeeded; target booted to Setup Assistant
- [x] **Obliteration verification hardware-verified**: an erase restore scans the device's `format_effaceable_storage` checkpoint (`result=0`) to confirm the encryption media key was destroyed; the verdict is surfaced (CLI + `Obliteration` event) and persisted to history. Reported on failure too, since the key can be destroyed before a later step fails
- [x] **`obliterate` command hardware-verified**: patched idevicerestore (`FLAG_OBLITERATE_ONLY`) stops the restore right after the effaceable wipe, leaving the Mac wiped and OS-less — a fast decommissioning wipe. Confirmed on an M1 MacBook Pro: the restore halted at `format_effaceable_storage result=0` and skipped the OS-image upload entirely (~34 prep checkpoints, no filesystem upload, vs a full restore)

## 10. Multi-device support & the Device primitive
- [x] `dfu::Target` selector (`One` / `Ecid`) unifying discovery: `dfu::find(target)` / `dfu::wait(target, timeout)` replace `find_one` / `wait_for_dfu`; ambiguity is an explicit `MultipleDevices` error
- [x] `dfu::watch()` — OS hotplug arrival watch (nusb `watch_devices`), subscribed *before* the DFU trigger so the entering Mac can't be missed; polling `list()` diff kept as backstop
- [x] CLI: `--ecid` (hex or decimal) on `restore`/`download`; interactive picker when several Macs are in DFU (TTY only — `--json`/non-TTY errors with an `--ecid` hint)
- [x] `run` merged into `restore` — one flagship command: triggers DFU entry if needed, then downloads and restores
- [x] Desktop app: already listed all devices and restored the selected one by serial/ECID; its `trigger_dfu` command now uses `dfu::watch()` and returns the Mac that newly entered DFU (the UI selects it), instead of grabbing the first DFU device after a refresh
- [x] Booted-Mac identity (cross-platform), via `device::identify`, matching what Apple Configurator shows:
  - **Model**: the USB `bcdDevice` release number BCD-encodes the model identifier's numeric part (`0x1701` → "17,1" → MacBookPro17,1; `0x1606` → Mac16,6), resolved against `MAC_MODELS` by unique numeric suffix. Available from enumeration alone — no open, fully cross-platform.
  - **ECID**: advertised in an Apple platform-capability descriptor inside the USB **BOS descriptor** (UUID `0a374ce4-…`, 8-byte little-endian payload — the same value macOS surfaces as `UsbAppleDeviceECID`, which is how Configurator reads it). Read with a standard `GET_DESCRIPTOR(BOS)` request via nusb (macOS/Linux/Windows); best-effort — a device we can't open keeps `ecid == None`, resolved for free at DFU.
  - Found by probing the device's descriptors directly (traced `cfgutil`/MobileDeviceKit → `MobileDevice.framework`) after two dead ends: reading identity over RemoteXPC is trust-gated at the RSD layer (proven with pymobiledevice3 on real hardware — needs a CoreDevice pairing+tunnel a third party can't reproduce), and the `UsbAppleDeviceECID` IORegistry property is macOS-only.
- [x] `device::Device` as the core primitive: `device::list()` enumerates every Apple USB device with its `UsbMode` (dfu/recovery/wtf/restore/booted/other — booted Macs enumerate as the RemoteXPC/NCM gadget, PID 0x1902, carrying the Apple serial; ECID filled in by `identify`) and restore-family identity; `Device::enter_dfu()` / `Device::reboot()` change modes (no-op if already in DFU; `UnsupportedHost` off Apple Silicon macOS — the VDM trigger acts on the host's DFU port, not an addressed device); `Target::Ecid` matches any mode, `Target::One` = the sole restorable (DFU) Mac; `dfu::` shrinks to the trigger, `list()` (restorable subset), and `watch`; desktop `list_devices` now maps `device::list()` (incl. Windows `driver_ready`) instead of enumerating USB itself
- [ ] Hardware-verify with two targets in DFU: picker, `--ecid`, and hotplug arrival on macOS/Linux/Windows

## 11. DFU-capable port detection (macOS)
- [x] `dfu/port.rs`: read-only IORegistry topology. `uart-hpm-rids` (device-tree bitmask of the HPM `RID`s carrying the UART/SWD/DFU debug harness — the same VMD path, per Asahi) declares which port controller(s) can trigger DFU; `vdm::find_device` now selects by that set (fallback `RID==0`) instead of hardcoding, hardening the trigger itself. Correlation: DFU controller's `port-number` → matching `usb-drd` USB controller → its `locationID` base → a device is on the DFU port iff `locationID & 0xff000000` matches. `port-location` gives the human name ("left-back").
- [x] `Device.port: Option<Port { dfu: bool, location: Option<String> }>` — the device's actual host port and whether it's DFU-capable — populated by `identify()` on macOS. Surfaced in CLI `list` ("left-front — move the cable to left-back to restore"), CLI `--json` (Device serializes), and the desktop (`DeviceView.port` + a card badge). `None` off macOS or when topology is unreadable — never a confidently-wrong answer. `dfu::dfu_port_label()` gives the DFU port's name.
- Verified live: `uart-hpm-rids` → RID 0 → "left-back" (base 0x00000000); a booted Mac on another port correctly reported "not the DFU port (move the cable to left-back)".

## Post-v1 follow-ups (not in this pass)
- [ ] Create github.com/fcjr/restorekit and push; add `TAP_GITHUB_TOKEN` secret (PAT with write access to fcjr/homebrew-fcjr)
- [ ] Code signing + notarization for the macOS binaries
- [ ] Tauri desktop UI over the library
- [ ] Windows packaging/testing
