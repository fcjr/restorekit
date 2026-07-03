# restorekit desktop — Product Requirements

## Summary

The restorekit desktop app is a macOS GUI over the `restorekit` library. It walks
a user through restoring a cabled Apple Silicon Mac visually — detect the target,
trigger DFU, download firmware, confirm, restore — without touching a terminal.

It reuses the exact engine the CLI uses. The `restorekit` library was built I/O-
free (every long operation reports progress through an `Event` callback) so a GUI
could sit on top of it directly. The Tauri backend calls the library and forwards
its `Event`s to the Svelte frontend as native events — no CLI subprocess, no
output parsing.

Built with **Tauri 2** (Rust backend) + **Svelte + Vite** (frontend).

## Goals

- A single-window app that takes a cabled target Mac from any state to a fresh
  macOS install, with clear status at every step.
- **One-click DFU** on capable hosts: an "Enter DFU" button that triggers DFU
  electronically (elevating just that step via the macOS admin prompt).
- Live device identification — show the exact model, chip/board, ECID, and iBoot
  the moment a Mac is detected in DFU.
- Firmware download with real progress, resumable, served instantly from the
  shared cache on repeat restores.
- An explicit, hard-to-fumble erase confirmation before any destructive action.
- Faithful restore progress and, on failure, the real underlying error text (the
  library already surfaces idevicerestore's error tail).

## Non-goals (v1)

- **macOS only.** The DFU trigger — the headline feature — only works on an Apple
  Silicon Mac host. A Linux/Windows GUI is out of scope for v1.
- **No bundled CLI.** The app and the `restorekit-cli` are distributed
  separately; the app doesn't drop a command on `PATH`.
- **Unsigned in v1.** Shipped without Apple notarization; the Homebrew cask
  strips the quarantine bit. Signing + notarization is a follow-up.
- No settings sprawl — the flow is opinionated, not a preferences panel.

## Privilege model

The DFU trigger opens the privileged `AppleHPM` IOKit user client and **requires
root** (enforced by `preflight()` in `crates/restorekit/src/dfu/vdm.rs`). A GUI
must not run as root, so:

- The app runs unprivileged. Detection, firmware resolution, download, and the
  restore itself all run in-process at the user's privilege level (USB/IOKit
  access and idevicerestore's restore phase don't need root on macOS).
- The **DFU trigger** is the one privileged action. It runs through a tiny
  bundled helper (`restorekit-dfu-helper`) invoked via the macOS admin prompt
  (`osascript … "with administrator privileges"`). The helper is the only code
  that ever runs as root — small and auditable, reusing the same VDM port.

> Open item: verify the restore truly runs unprivileged on current macOS. If it
> needs root, route it through the elevated helper too (with an explicit cache
> dir in the user's home so the ~13 GB firmware stays user-readable).

## User flow

```
      ┌─────────┐   Enter DFU (elevated)   ┌──────────┐
      │  Idle   │ ───────────────────────▶ │ Detected │
      │ (empty) │ ◀─── manual DFU ──────── │          │
      └─────────┘                          └────┬─────┘
                                                │ Restore
                                                ▼
   ┌──────────┐   confirm    ┌───────────┐   run   ┌───────────┐
   │ Download │ ───────────▶ │  Confirm  │ ──────▶ │ Restoring │ ─▶ Done
   │ progress │              │  (erase)  │         │  progress │ ─▶ Error
   └──────────┘              └───────────┘         └───────────┘
```

1. **Idle** — no device. If the host can trigger DFU, an *Enter DFU* button;
   otherwise the manual DFU key-combo for the detected chassis.
2. **Detected** — the device card: model, identifier, chip/board, ECID, iBoot,
   and a primary *Restore* button.
3. **Download** — firmware resolves, then downloads with a progress bar (bytes +
   ETA). A cached, verified firmware skips straight through.
4. **Confirm** — a modal naming the exact model + ECID; the user types to confirm
   before any erase.
5. **Restoring** — step name and percentage; "do not disconnect the target."
6. **Done** — "restored — booting to Setup Assistant." **Error** — the real
   failure text, with a retry.

## Screen states

| State | Shows | Primary action |
| --- | --- | --- |
| Idle | "Connect a Mac in DFU mode" | Enter DFU / manual instructions |
| Detected | Device card (model, ECID, iBoot) | Restore |
| Resolving | "Finding firmware…" | — |
| Downloading | Progress (bytes, ETA), cache-hit fast path | Cancel |
| Confirm | Erase warning (model + ECID) | Type-to-confirm |
| Restoring | Step + percent, "don't disconnect" | — |
| Done | Success | Restore another |
| Error | Real error text | Retry |

## Functional requirements

- Poll for a DFU device (~2s) and reflect connect/disconnect live.
- Map every library `Event` (`DfuTriggerStage`, `DownloadProgress`, `Verifying`,
  `RestoreStep`, `Done`) to a UI update via a single Tauri event channel.
- Elevate only the trigger; surface a clear error if the admin prompt is denied.
- Never start a restore without an explicit confirm (mode is Erase by default;
  Revive is a distinct, non-destructive choice).
- Use the same shared cache as the CLI
  (`${XDG_CONFIG_HOME:-~/.config}/restorekit/firmwares`).

## Design

Reuses restorekit's console identity: cool graphite neutrals, a single amber
signal, one muted "alive" green for the done state, San Francisco / SF Mono
(system faces — no web fonts). The device card and progress read like precise
instrument output, not a generic dashboard.

## Requirements & dependencies

- macOS 12+ on Apple Silicon (host).
- Bundles `restorekit-dfu-helper` as a Tauri `externalBin`.
- Links the `restorekit` library, which statically links the idevicerestore C
  stack — the app is self-contained (no external tools).

## Risks

1. **Restore may need root** — see the privilege-model open item.
2. **osascript elevation + TCC** — breaks the attribution chain; acceptable for a
   one-shot trigger, but a hardened `SMAppService` helper is the follow-up.
3. **Unsigned app** — Gatekeeper friction, mitigated by the cask quarantine hook;
   notarization is the real fix.
4. **Private AppleHPM API** — a macOS update could change it; the trigger then
   fails gracefully and the app falls back to manual DFU instructions.

## Distribution

`brew install --cask restorekit` (the plain cask token is the app; the CLI is
`--cask restorekit-cli`). Built and released by `tauri-apps/tauri-action` on a
version tag. See [DEPLOYMENT.md](DEPLOYMENT.md).
