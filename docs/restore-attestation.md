# Restore attestation: what it is, and why restorekit can't (and needn't) do it

This documents an investigation into Apple Silicon **restore attestation** — the
`RestoreAttestation` step in the DFU restore protocol. Short version: an attested
restore requires the *host* to prove it's an Apple-account-authenticated station,
which only Apple's own tooling can do; a normal restore doesn't need it; and it
was never relevant to verifiable erasure anyway. It's written down so nobody has
to re-run the whole dead end.

## TL;DR

- A restore attestation involves **two** parties: the **device** attests it's
  genuine, and the **host** attests it's an authorized restore "station."
- The device half is doable (idevicerestore can trigger it). The **host half is
  gated behind Apple's AuthKit + Anisette account infrastructure** (`akd`,
  `cloudd`), so only Configurator/MobileDevice on an Apple-ID-signed-in Mac can
  produce it.
- Restoring **does not require attestation** — restorekit restores fine with it
  declined, which is what it does.
- The attestation attests device identity + installed OS/firmware measurements.
  It is **not** a proof of erasure and is verifiable only by Apple.
- Net for refurbishment: **no value.** Use the effaceable-wipe checkpoint (see
  [obliterate.md](obliterate.md)) for verifiable erasure.

## Background

While building verifiable erasure, we noticed the restore protocol has a
`RestoreAttestation` message that stock idevicerestore explicitly declines
(`RestoreShouldAttest: false`). We wanted to know what it is and whether it
could give a stronger, signed proof than the device's self-reported checkpoints.

## The two-party attestation architecture

### 1. Device attestation (BAA)

When the host opts in (`RestoreShouldAttest: true`), the device produces a
hardware attestation and returns it as `RestoreAttestationState: AttestationIssued`
with an X.509 cert chain:

- **Leaf**: `CN=<measurement digest>, OU=BAA Certification, O=Apple Inc.`, ~3-day
  validity. Its Apple OID extensions (`1.2.840.113635.100.8.x`) carry the device
  identity (ChipID `8103`/M1, ECID), the OS version being installed, and firmware
  measurements (SEP `rsep`, iBoot `ibss`, etc.).
- **Chain**: `Basic Attestation System Sub CA1` → `Basic Attestation System Root CA`
  (Apple's "BAA" — Basic Attestation Authority PKI).

The device fetches this from Apple's `humb.apple.com` **through the host's reverse
proxy** (the device has no network in restore mode; the ramdisk's
PurpleReverseProxy tunnels its SOCKS connections out through the host). This half
works with idevicerestore — we captured a real cert (`~/restorekit-attestation/`).

### 2. Host / "station" attestation (the SCRT)

The device won't commit the recovery-OS **LocalPolicy** on an attested restore
unless the *host* also presents a valid attestation — a "station-local SCRT"
(Secure Cryptographic Restore Ticket) rooted in Apple's **`Apple Accessory Host
Attestation Authority`**. This is the piece idevicerestore doesn't do.

Reading Apple Configurator's `cfgutil` restore via the unified log showed exactly
how the host produces it:

```
(MobileDevice) Restore attestation requested because build identity asked for attestation
(MobileDevice) Restore security attestation: AVAILABLE
cloudd (AuthKit) authkit/fetch-attestation-data-async
[com.apple.authkit:signpost] BEGIN FetchAttestation
akd 'attestationDataForRequestData:completion:'
akd BEGIN SignAndAttestation enableTelemetry=YES
cloudd [com.apple.authkit:traffic] Remote Anisette service returned Attestation data
(MobileDevice) Attestation manifest hash matches
(MobileDevice) Restore certificate authenticated DFU attestation
```

So the host attestation is produced by **AuthKit** (`akd`, the Apple-ID daemon,
and `cloudd`), fetching its data from **Apple's Anisette service** — the same
per-machine + per-account anti-fraud identity machinery used for Apple-ID/iCloud
authentication. It is bound to a **signed-in Apple ID** on the host and Apple's
live Anisette provisioning.

## Why restorekit / idevicerestore can't complete it

We reproduced the failure precisely. With attestation enabled:

1. The device attests fine (fetches its BAA cert via the reverse proxy).
2. At `macos_create_recovery_local_policy`, the host sends the **identical**,
   valid, Apple-TSS-signed LocalPolicy it sends on a normal restore (verified by
   dumping the request — the arguments are byte-for-byte the same three fields).
3. The device **rejects** it with `result=6 (failed to create recovery os local
   policy)` — accepted when not attested, rejected when attested.

The rejection isn't about the LocalPolicy contents or any missing protocol field
(we tried `AttestationUseSCRT` true and false, and enabling the `Provisioning*`
messages — none change it). The device requires the host's **station attestation**,
and idevicerestore performs none: no AuthKit, no Anisette, no Apple-ID session.

Making restorekit do it would mean reimplementing Apple-account attestation —
an Apple-ID login, Apple's private AuthKit attestation APIs, and Anisette
provisioning (the thing AltServer/SideStore proxy just for basic Apple-ID
sign-in). That is macOS-host-only, account-gated, version-fragile, and squarely
inside Apple's authentication infrastructure. It is not a "thread a field
through" fix; it's a different product. This is an **intentional gate**: only an
Apple-account-authenticated station may perform an attested restore.

## What restore attestation is actually for

It's a trust / anti-abuse mechanism, not a functional requirement. Requiring
*both* a genuine device and an authenticated, accountable host lets Apple gate
security-sensitive restore operations — chiefly **creating the recovery-OS
LocalPolicy**, which defines boot/ownership/sealing policy. That underpins
Activation Lock / ownership enforcement, provisioning and MDM/fleet trust, and
anti-fraud (the Anisette tie-in binds mass operations to real accounts). It's
about controlling *who may reconfigure a device's security posture*.

## Why it doesn't matter for refurbishment

- **Not required to restore.** restorekit declines attestation and restores fine.
- **Not required to wipe.** `obliterate`/erase work with zero attestation.
- **Not a proof of erasure.** The cert attests install + identity + firmware
  measurements — never "the media key was destroyed" — and it's Apple-only-
  verifiable and expires in ~3 days.

For verifiable erasure, the real signal is the device's `format_effaceable_storage`
checkpoint (`result=0`), which restorekit already captures and records. See
[obliterate.md](obliterate.md).

## Evidence

- `~/restorekit-attestation/` (on the dev machine, not in-repo): the captured
  device attestation — `cert0.der` (leaf), `cert1.der` (sub CA), `attestation.plist`
  (the raw `AttestationIssued` message), `cert0.txt` (decoded), and the restore
  logs.
- Host-side flow: captured from `cfgutil restore` via
  `log stream` filtered to the attestation/activation subsystems.

## Conclusion

Restore attestation is Apple's two-party (device + Apple-account-authenticated
host) trust gate for security-policy creation during restore. It is not
completable outside Apple's authorized, account-authenticated tooling, and it is
orthogonal to both restoring and to verifiable erasure. The research patches used
to probe it (an attestation opt-in and a LocalPolicy-request dump) were removed
after this investigation; they are recoverable from git history if ever needed.
