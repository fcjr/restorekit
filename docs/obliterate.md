# How `obliterate` works

`restorekit obliterate` destroys a Mac's encryption key and stops — it wipes the
machine without spending the time (or bandwidth) to reinstall the OS. On the
hardware we tested it completes in about **30 seconds**, versus 15–30 minutes for
a full `restore`.

This document explains the security model it relies on and exactly what the
command does.

## The security model: erase by destroying the key

On Apple Silicon and T2 Macs, **all user data on the internal storage is always
encrypted**. Each volume is encrypted with a volume key, and those keys are
ultimately wrapped by a hardware **media key** held in a small region of storage
called **effaceable storage**, managed by the Secure Enclave.

Because every byte is already encrypted under that key, you don't have to
overwrite the data to erase it — you destroy the media key. Once the media key is
gone, the wrapped volume keys can never be recovered, so the ciphertext on the
drive is permanently unreadable. Apple calls this cryptographic erasure, and it's
the same primitive behind "Erase All Content and Settings."

- Apple Platform Security — Data Protection overview:
  <https://support.apple.com/guide/security/data-protection-sece8608431d/web>
- Apple Platform Security — Secure Enclave / effaceable storage:
  <https://support.apple.com/guide/security/secure-enclave-sec59b0b31ff/web>

`obliterate` performs exactly this key destruction and nothing more.

## What the command does

A normal erase restore boots a restore ramdisk over DFU, wipes the effaceable
storage early on, and *then* spends most of its time writing the OS image back.
`obliterate` runs that same flow but **stops the moment the key is destroyed**,
before the OS write:

1. Trigger DFU entry on the target (via the dongle, or the host on Apple Silicon
   macOS).
2. Personalize and boot the restore ramdisk — identical to an erase restore up to
   this point.
3. The device runs its restore sequence and reports a **checkpoint** to the host
   for each step. When it completes the effaceable-storage format —

   ```
   Checkpoint completed id: 0x61F (format_effaceable_storage) result=0
   ```

   — the media key has been destroyed (`result=0` means success).
4. restorekit stops the restore cleanly at that checkpoint instead of continuing
   to the OS upload. The device is left wiped and OS-less.

Everything before step 3 is byte-for-byte a normal erase restore; the only
difference is that `obliterate` bails out at the wipe checkpoint.

## How the stop is implemented

restorekit vendors and patches [idevicerestore](https://github.com/libimobiledevice/idevicerestore).
Patch `0003` (see `crates/restorekit-sys/patches/idevicerestore/`) adds a
`FLAG_OBLITERATE_ONLY` restore flag. In the restore message loop, when that flag
is set and the `format_effaceable_storage` checkpoint completes, it sets the
existing `FLAG_QUIT` — the same mechanism a normal successful restore uses to end
its loop — so the restore returns cleanly right after the wipe. A non-zero
checkpoint result is treated as a failed wipe and aborts with an error.

`Mode::Obliterate` sets `FLAG_ERASE | FLAG_OBLITERATE_ONLY`. It's a normal erase
as far as the device is concerned; the host just refuses to go any further once
the key is gone.

## Verification

The wipe verdict is derived by scanning the restore log for that
`format_effaceable_storage` checkpoint and its result code:

- `result=0` → `confirmed` (key destroyed)
- non-zero → `failed`
- checkpoint never seen → `unconfirmed`

For `obliterate`, a `confirmed` verdict is the successful outcome. The verdict is
surfaced in the CLI output and the `obliteration` progress event, and recorded to
the history log. Because the key is destroyed inside the Secure Enclave, this
checkpoint attestation is the strongest confirmation available — the key value
itself is never readable, by design.

## End state and caveats

- After `obliterate` the Mac is **wiped and has no OS**. It sits in restore mode
  and falls back to DFU. Run `restorekit restore` to reinstall macOS and make it
  usable again.
- Obliteration needs Apple's signing server (TSS) online at wipe time to boot the
  personalized ramdisk — it is not an offline operation.
- The target's data is unrecoverable the instant the checkpoint reports
  `result=0`, whether or not the tool later reports success.

## Usage

```sh
# Destroy the key and stop (leaves the Mac wiped, no OS)
sudo restorekit obliterate --yes

# Time it
sudo -v && time sudo restorekit obliterate --yes

# Reinstall an OS afterward
sudo restorekit restore --yes
```

In the desktop app, pick **Obliterate** in the restore Mode selector; the confirm
dialog spells out that no OS is reinstalled.
