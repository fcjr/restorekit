# Thunderbolt restore: what it is, why restorekit can't drive it, and what we built instead

This documents an investigation into restoring an Apple Silicon Mac over
**Thunderbolt** instead of USB — the "super speed" restore path. Short version:
the fast transport is real in the *device* firmware, but the host side lives
entirely inside Apple's private, entitled `MobileDevice.framework`; there is no
public API to drive it, the device gates it behind the same account-attested
"authorized transport" wall as restore attestation, and **Apple's own `cfgutil`
restores over USB anyway** — so there is nothing for a third-party host to talk
to. It is written down so nobody has to re-run the dead end.

## TL;DR

- The device's restore stack supports a Thunderbolt/USB4 data transport (Apple
  Configuration I/O — **ACIO/CIO**), gated by a **Transport Restriction Manager
  (TRM)** and a "Restore Authorized Transport" grant.
- The **host** half is not reachable: macOS exposes **no public IOKit
  Thunderbolt data-transport API**, and idevicerestore/libimobiledevice are
  USB-only (usbmux over USB) with no transport abstraction.
- The TRM grant is **Apple-account-gated**, the same way restore attestation is
  (see [restore-attestation.md](restore-attestation.md)).
- **Empirically, `cfgutil` restored a DFU Mac over USB** — the ACIO transport
  was never engaged (confirmed from the host's unified log). Apple's own tool
  doesn't use it for a DFU restore, so even a perfect host implementation would
  have no peer.
- Net: a working Thunderbolt restore **can't be built** in restorekit. What we
  shipped is a device-side **probe** — a `--boot-args` passthrough — not a fast
  transport.

## Background

Prompted by the idea that the restore could be pushed into "Thunderbolt speeds,"
we unpacked `UniversalMac_26.5.2_25F84_Restore.ipsw` and searched the restore
kernelcache. The mechanism is genuinely there: the restore kernel carries
`"Restore Authorized Transport"`, the ACIO/CIO Transport driver stack, the TRM,
and boot-arg/property hooks (`acio`, `disable-transport-rm`). So the *device* can
speak restore over Thunderbolt. The question was whether a host could drive it.

## The three walls

Moving the restore from USB to Thunderbolt breaks into three layers. Each is
independently blocking.

### 1. Host transport — no public Thunderbolt data API

restorekit's restore protocol (`restored`) rides usbmux over USB. To move it to
Thunderbolt the host would have to open an ACIO/CIO data channel to the target.
macOS ships **no public IOKit interface** for arbitrary Thunderbolt data
transport (there are no Thunderbolt headers in `IOKit.framework`). The ACIO
transport is brought up by the kernel Thunderbolt stack and is reachable only by
**entitled Apple frameworks** — in practice `MobileDevice.framework`, which
Configurator/cfgutil links. A userspace tool cannot open it. idevicerestore has
no code for it at all: no ACIO, no transport abstraction, USB only.

### 2. Device authorization — the TRM gate

Even with a host channel, the device won't route restore data over Thunderbolt
unless its **Transport Restriction Manager** grants "Restore Authorized
Transport." That grant is bound to an **authenticated restore station** — the
same Apple-account (AuthKit/Anisette) machinery that gates restore attestation.
A tool with no Apple-ID session and no attestation can't obtain it. This is the
identical wall documented in [restore-attestation.md](restore-attestation.md),
seen from the transport side.

### 3. Reality — cfgutil restores over USB

The decisive finding. We restored the target Mac with Apple's own `cfgutil` and
read the host's unified log across the whole restore window. The ACIO transport
was **never activated** — no `setCIOTransportActive`, no "Restore Authorized
Transport," no ACIO data path. The only Thunderbolt-controller log lines were
ordinary cable-state noise. **Apple's own tool restored the DFU Mac over USB.**

That means the Thunderbolt "authorized transport" is not part of the consumer
DFU-restore flow at all — it belongs to an internal/manufacturing scenario with
a fixture that authorizes it. A third-party host implementation would have no
peer to negotiate with, because the target never offers the transport during a
normal DFU restore.

## Why the USB ceiling is what it is

DFU restore is fundamentally a USB gadget: the target enumerates as a USB device
(DFU → recovery → restore mode) and the restore daemon speaks over that link. On
Apple Silicon this is USB 2.0-class for the restore transfer, which is why the
filesystem (ASR) upload dominates wall-clock regardless of `cfgutil` vs
restorekit. Thunderbolt would lift that ceiling — but only via a path neither
tool can reach, and that Apple's tool doesn't take either.

## What we built instead: the boot-args probe

The one honest, buildable artifact here is a **device-side probe**, not a fast
transport. restorekit can now append arbitrary boot-args to the restore ramdisk
kernel via `--boot-args`, including the transport-restriction hooks:

```sh
# Experimental: push transport-restriction boot-args into the restore ramdisk.
# This does NOT move the restore onto Thunderbolt — the host stays on USB.
sudo restorekit restore --boot-args "disable-transport-rm" ...
```

Implementation:

- idevicerestore patch `0005` (`crates/restorekit-sys/patches/idevicerestore/`)
  adds a `restore_boot_args_extra` field and an
  `idevicerestore_set_boot_args()` setter. `recovery_enter_restore` appends the
  extra args to the default `restore_boot_args` before the `setenv boot-args`
  that iBoot applies when booting the ramdisk.
- restorekit threads `boot_args: Option<&str>` through `restore()` and exposes
  it as the hidden, experimental `--boot-args` flag.

What it is good for: empirically poking Wall 2 — booting the restore ramdisk with
`disable-transport-rm`/`acio` set and watching whether device behavior changes.
What it is **not**: a Thunderbolt restore. The host has no ACIO channel (Wall 1),
so the restore data still goes over USB no matter what the device does. The flag
is hidden and documented as experimental for exactly this reason.

## Conclusion

Thunderbolt restore is real in Apple's device firmware but sits behind an
entitled, account-attested host path that no third-party tool can drive, and that
Apple's own `cfgutil` does not even use for a DFU restore. There is no plist
toggle or cfgutil flag that switches a DFU restore onto Thunderbolt; the
selection happens inside `MobileDevice.framework` and, in practice, chooses USB.
For refurbishment throughput the lever is not the transport — it's avoiding the
OS write entirely, which is what [obliterate.md](obliterate.md) does.

The probe patch is small and self-contained; if a future macOS ever exposes the
transport, or an internal procedure to authorize it surfaces, `--boot-args` is
the hook already in place to experiment from.
