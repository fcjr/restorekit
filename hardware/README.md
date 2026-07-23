# RecoverKit dongle hardware

Two variants of the same board: an inline USB dongle that sits between a host
computer and an Apple Silicon / T2 Mac and puts the target into DFU without
anyone touching the target's keyboard. `dongle-lite/` is the USB 2.0 version,
`dongle-pro/` adds a 5 Gbps SuperSpeed passthrough. Each directory has its own
PRD and README; this page explains what they share and where they differ.

## What both boards do

The platform is identical on both:

- **RP2354A MCU** enumerates to the host (shared VID:PID `16D0:14F0`) as a
  composite device: a control channel for `restorekit`, a CDC serial port, and
  the DFU-trigger state machine. Firmware is one crate,
  [`crates/dongle-lite-fw`](../crates/dongle-lite-fw).
- **FUSB302B USB-PD PHY** owns the *target* port's CC line. To force DFU it
  runs Apple's vendor-defined message sequence over USB-PD — the same thing a
  second Mac does when you use Apple Configurator, minus the second Mac.
- **AP22653 load switch** gates target VBUS so the dongle can power-cycle the
  target port for clean re-enumeration.
- **SBU serial block** (2× 74AVC1T45 + 1.2 V rail) exposes the target's SBU1/2
  debug UART.
- A **USB hub** between host and target carries the actual restore traffic:
  the host sees two devices behind the dongle — the MCU and the target Mac.

So a restore looks like: host talks to the MCU → MCU VDM-triggers DFU on CC →
target re-enumerates through the hub as a DFU device → the host restores it
directly. The dongle never proxies restore data; it only wires the paths and
flips the target's state.

## Lite vs Pro

|  | **Dongle-Lite** | **Dongle-Pro** |
|---|---|---|
| Data path | USB 2.0 (CH334F hub) | USB 3.1 Gen 1, 5 Gbps (GL3510 hub) + USB 2.0 |
| Receptacles | 16-pin USB-C (2.0-only pinout) | 24-pin full-featured USB-C |
| Host CC | 5.1 k Rd pulldowns | HD3SS3220 (autonomous CC + SS 2:1 mux) |
| Target SS mux | — | HD3SS3212, SEL driven by firmware from CC polarity |
| SS ESD | — | 4× TPD4E05U06 |
| Extra rails | — | GL3510 internal 3.3 V LDO + 1.2 V buck |
| Firmware build | default | `--features pro` |
| iProduct / serial | `Dongle-Lite` / `DL-…` | `Dongle-Pro` / `DP-…` |
| Update asset | `dongle-lite-fw.bin` | `dongle-pro-fw.bin` (same release tag) |

**The part that surprises people:** the Pro does *not* restore faster. Apple
hard-caps the target side of a DFU restore at USB 2.0 (FB13555999), so DFU and
restore traffic run over the hub's USB 2.0 path on both boards. The Pro's
SuperSpeed path benefits a *normally booted* target — file transfer, target
disk mode, anything that negotiates USB 3 — and degrades gracefully to the
Lite's behavior with a USB 2.0 cable.

```
 Lite:  Host ── CH334F hub ──┬── USB2 ── Target
                             └── USB2 ── RP2354A
        FUSB302B ── CC ────────────────── Target   (DFU trigger)

 Pro:   Host ── HD3SS3220 ── SS ── GL3510 ── SS ── HD3SS3212 ── Target
              └──────────── USB2 ── GL3510 ──┬─── USB2 ──────── Target
                                             └─── USB2 ──────── RP2354A
        FUSB302B ── CC ────────────────────────────────────────── Target
```

Two-CC-owners rule on the Pro: the HD3SS3220 autonomously owns the **host**
CC (and steers its own SS mux on plug flip); the FUSB302B remains the *only*
thing driving the **target** CC, so the Apple VDM flow is byte-identical on
both boards. The target-side HD3SS3212 never touches CC — it is a data-only
lane mux whose SEL pin the firmware sets from the FUSB302B's reported plug
orientation (GP26, high = CC1/normal).

## Telling them apart in software

Same VID:PID, distinguished by USB descriptors: iProduct string and serial
prefix (see table). The host SDK maps this to `DongleModel::{Lite,Pro}`
(`crates/restorekit-dongle-proto`), and the firmware updater selects the
per-model asset from the shared `dongle-lite-v*` release tag — a Pro can
never be flashed with a Lite image by accident.

## Layout

Both are 4-layer JLCPCB boards with single-sided assembly and generated,
reproducible layouts (netlist → placement → routing → pours, all scripted
under each variant's `gen/`). The Pro additionally carries 90 Ω
impedance-controlled differential pairs over a solid inner ground plane —
order it with the impedance-control option (see
`dongle-pro/mfg-jlcpcb/README-JLCPCB.md`). Fab packages (gerbers, BOM, CPL)
live in each variant's `mfg-jlcpcb*/` directory.
