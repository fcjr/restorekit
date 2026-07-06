# restorekit — Product Requirements

## Summary

`restorekit` is a cross-platform CLI (and Rust library) for restoring Apple
Silicon Macs over USB. It automates the full recovery workflow that today
requires Apple Configurator 2 and manual steps:

1. Put a target Mac into DFU mode over a USB-C cable (no keyboard gymnastics),
   using the USB-PD vendor-defined-message technique pioneered by
   [macvdmtool](https://github.com/AsahiLinux/macvdmtool).
2. Detect the Mac in DFU mode and identify its exact model.
3. Automatically resolve and download the correct macOS IPSW firmware for that
   model, cached locally.
4. Restore (erase) or revive the target by driving
   [idevicerestore](https://github.com/libimobiledevice/idevicerestore).

The core is a reusable library crate (`restorekit`) with a thin clap CLI
(`restorekit-cli`) on top, so a GUI (e.g. Tauri) can be layered on later
without touching the core.

## Goals

- One-command restore: `restorekit restore` takes a cabled, powered-on target
  Mac from any state to a fresh macOS install.
- DFU triggering from an Apple Silicon macOS host via USB-PD VDM (requires
  root; DFU-capable port on both ends).
- DFU detection and model identification on macOS and Linux, with no daemons
  or drivers beyond stock OS facilities.
- Automatic firmware resolution: DFU device → chip/board IDs → model →
  latest signed IPSW (or a user-pinned version).
- Resumable, checksum-verified downloads cached in
  `${XDG_CONFIG_HOME:-~/.config}/restorekit/firmwares` (override with
  `--cache-dir` or `RESTOREKIT_CACHE_DIR`).
- Library-first design: no printing or prompting inside the library; all
  progress flows through a typed event callback (CLI renders progress bars,
  `--json` emits machine-readable events).
- Distribution: Homebrew (`brew install fcjr/fcjr/restorekit`) plus GitHub
  release tarballs for Linux, built and published by goreleaser.

## Non-goals (v1)

- No Apple Configurator / `cfgutil` integration or any macOS-only restore
  fallback — `idevicerestore` is the sole restore engine on every platform.
  The only macOS-only component is the DFU trigger.
- No Intel/T2 support (neither as host nor target).
- No Windows builds (the library is written to be portable; packaging and
  testing are out of scope for v1).
- No IPSW mirroring/redistribution — firmware is always fetched from Apple's
  CDN.
- No GUI (planned as a Tauri app in a later milestone, reusing the library).

## Users & flows

**IT technician / refurbisher** — bench setup with a host Mac:
`sudo restorekit restore` → target reboots into DFU, firmware resolves from
cache, restore runs to 100%, target boots to Setup Assistant. With several
targets in DFU, an interactive picker (or `--ecid` for scripting) selects one.

**Developer / tinkerer with a bricked Mac and a Linux box:** puts the target
into DFU manually (restorekit prints the key-combo instructions), then
`restorekit restore` detects, downloads, and restores.

**Cautious user:** `restorekit list` → `restorekit download` →
`restorekit restore` as separate, inspectable steps.

## CLI surface

| Command | Behavior |
|---|---|
| `list` | List every connected Apple device with its mode and ECID (booted Macs' ECID on macOS hosts). |
| `dfu` | Reboot the cabled target into DFU (macOS AS host, root). |
| `reboot` | Reboot the cabled target normally (undo a DFU trigger). |
| `download` | Resolve firmware for the detected DFU device (or `--identifier`), download to cache. |
| `restore` | One-shot erase-restore: trigger DFU entry if needed → wait → download → full erase-restore (`--yes` skips confirmation; `--ipsw`/`--os-version` pin firmware; `--ecid` picks a target when several are in DFU, otherwise an interactive picker). |
| `revive` | Reinstall firmware without erasing user data (un-brick after a failed update). Same target/firmware flags as `restore`; no erase, so no `--yes`. |
| `cache` | Show or clear the firmware cache (`--path`, `--clear`). |
| `setup-driver` | Bind the WinUSB driver so restorekit can reach the target (Windows only). |

Global flags: `--cache-dir`, `--json`, `-v`.

## Platform matrix

| Capability | macOS (Apple Silicon) | macOS (Intel) | Linux |
|---|---|---|---|
| DFU trigger (`dfu`/`reboot`) | ✅ (root) | ❌ manual instructions | ❌ manual instructions |
| DFU detection / identify | ✅ | ✅ | ✅ |
| Firmware resolve + download | ✅ | ✅ | ✅ |
| Restore (idevicerestore) | ✅ | ✅ | ✅ (needs usbmuxd) |

## External dependencies

- **idevicerestore** is statically linked into the binary from pinned vendored
  sources (`restorekit-sys`) — no external tools required. (Originally planned
  as a `$PATH` dependency; superseded by the FFI build in plan §6.)
- **ipsw.me v4 API** for firmware metadata (no auth), with Apple's official
  `mesu.apple.com` macOS IPSW plist feed as fallback resolver.
- Firmware payloads download directly from Apple's CDN.

## Risks

1. **idevicerestore on Apple Silicon Macs is community-proven, not officially
   documented.** Mitigation: version pinning flags, loud failure surface with
   the tail of the restore log.
2. **The AppleHPM user client is private Apple API** and could change in a
   macOS update. Mitigation: behavior pinned to macvdmtool upstream; failures
   degrade to manual DFU instructions.
3. **Restores are destructive.** Mitigation: the erasing `restore` command
   requires an explicit model+ECID confirmation prompt (`--yes` for
   non-interactive erase); the non-destructive path is a separate `revive`
   command that never erases, so the two intents can't be confused.
4. **ipsw.me is community-run.** Mitigation: mesu fallback + `--ipsw` for
   fully offline operation.

## Success criteria

- A target Mac cabled to an AS host restores end-to-end with a single command.
- The same binary on Linux detects a DFU Mac and restores it (manual DFU entry).
- `brew install fcjr/fcjr/restorekit` works on a clean machine.
- Firmware for a repeat restore of the same model is served from the cache with
  zero re-download.
