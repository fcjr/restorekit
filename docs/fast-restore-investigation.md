# Faster Apple Silicon restore: the USB-2 ceiling and how to break it

**Goal:** make the DFU restore faster than the USB-2 (480 Mbps) ceiling that
dominates the ASR filesystem upload (~7.5 min of a ~11 min restore).

**Hardware under test:** M1 MacBook Pro (CPID `0x8103`, board **j293**),
host-based DFU from an Apple Silicon Mac, OWC Thunderbolt 4 cable.

---

## Bottom line

The restore gadget runs at USB-2 **not** because the hardware or kernel can't do
SuperSpeed — both can — but because **iBoot strips the boot-arg that would enable
it.** The full chain, all disassembly-proven:

1. **The kernel will do SuperSpeed.** `force-usb-superspeed` is an *unconditional*
   override that programs the DWC3 device controller straight to SS. No restore
   gate, no device-tree cap. (§1)
2. **iBoot strips it.** iBEC filters `boot-args` against a signed allowlist before
   the kernel runs. The allowlist is trace/debug args only — `force-usb-*` is not
   on it, so it never reaches the kernel. (§2)
3. **There is a bypass, gated by security mode.** iBEC skips the allowlist when
   `allow-whitelist-disable = 1` (already set in the restore DT) **and** the AP is
   in a permissive security state. That state is set by
   `bputil --disable-boot-args-restriction` — but that's a **LocalPolicy change,
   which the SEP only signs with owner authentication.** (§3)

**Practical status:**
- On a *stock production* M1, faster-than-USB-2 restore is **not reachable** — the
  boot-arg is cryptographically filtered.
- On an M1 in **Permissive Security**, `force-usb-superspeed` should pass. One
  open question remains — whether the DFU-restore *ramdisk* boot honors the
  installed permissive policy — decided by the test in §4.
- **Without logging into the Mac: not possible.** Permissive security requires
  owner auth (SEP-signed LocalPolicy); M1 has no bootROM exploit to sidestep it.
  See §5 for what this means for a refurbish workflow.

A "4-minute M1 restore" is consistent with this: an M1 already in Permissive
Security, restored with `force-usb-superspeed`, would train an SS link and collapse
the ~7.5 min of upload to ~45 s.

---

## §1 — The kernel will present SuperSpeed (proven)

Disassembled the speed path in the j293 (mac13g) restore kernelcache.

`IOUSBDeviceController::initSpeedOverride` (`0xfffffe000ad74d6c`,
`com.apple.iokit.IOUSBDeviceFamily`):
- `PE_parse_boot_argn("force-usb-superspeed", …)`; if present and non-zero, sets
  ivar `_speedOverride` (`+0xc0`) = **3 (SuperSpeed)**, **unconditionally** — no
  restore-mode flag, no boot-env check, no DT read, no clamp. The only in-function
  branches select a logger. (`force-usb-highspeed`→2, `force-usb-fullspeed`→1.)
- Hardware write in `AppleUSBXDCI` (`sub_…adb5124`, the DCFG programmer):
  `_speedOverride==3` → writes DWC3 `DCFG.DEVSPD[2:0] = 4` (SuperSpeed), unclamped.
  With **no** boot-arg (`_speedOverride == -1`) it writes nothing, so DCFG keeps
  its **reset default = High-Speed**. That is why restores are USB-2 by default.
- The only theoretical veto is a DT `usb-max-speed-capability` below SS on
  `usb-drd0` — and even that only feeds a *reporting getter*, not the DCFG write.
  Driver default is 4 (SuperSpeed).

The j293 DeviceTree has **no `usb-max-speed-capability` on `usb-drd0`**, so the SS
default stands. (`usb-drd0` is the restore port; `usb-drd1` carries
`usb-restore-disable`, drd0 doesn't. `usb-drd0` connects via `atc-phy-parent`, the
Type-C combo PHY, whose SS lanes iBEC programs — `tunable_USB_LN0/LN1_AUSPMA_*`.)

**Nothing in software or the device tree caps the restore gadget at USB-2.**

## §2 — iBoot strips the boot-arg (proven)

The `force-usb-fullspeed` test settled where delivery breaks. It's an
unconditional override (§1) that forces **USB-1, 12 Mbps** — so it's observable
from the host. Result:

```
DFU Mode        spd=2  high-480M
Recovery Mode   spd=2  high-480M
MacBook Pro     spd=2  high-480M   ← restore ramdisk, STILL 480, NOT 12 Mbps
```

The speed didn't change → **the kernel never received the arg.** (This retroactively
invalidates every earlier USB-3 experiment — `hpmAllowUsb3Restore`,
`force-usb-superspeed`: none of those args ever reached the kernel either.)

Our code sends it correctly — verified end to end:
- CLI `--boot-args` → `idevicerestore_set_boot_args` (strdup →
  `restore_boot_args_extra`) → `recovery_enter_restore` appends to
  `restore_boot_args` → `recovery_send_kernelcache` issues
  `setenv boot-args rd=md0 nand-enable-reformat=1 -progress -restore <extra>` →
  `bootx`. The boot-args patch was confirmed applied in the built copy (since
  removed); `set_boot_args` ran
  before `idevicerestore_start`.
- Flow: DFU → `dfu_enter_recovery` (auto-boot=false) → MODE_RECOVERY →
  `recovery_enter_restore` → `setenv boot-args … ; bootx`. That `setenv` *is* the
  ramdisk kernel's boot-args, and it includes our extra.

So the strip is in **iBEC**. Disassembled it (capstone, flat arm64):
- `getenv("boot-args")` → tokenizer `0x811cc` → per-token allow/deny `0x81850`. A
  token survives only if it matches a name in the comma-separated allowlist.
- The allowlist is the `/chosen` `allowed-boot-args` property, built by iBoot from
  the Apple-signed manifest — not nvram, not user-settable. Read from the j293
  restore DT:

  ```
  allowed-boot-args     = trace,trace_wake,kperf,-x,-v,trm_enabled,
                          trace_typefilter,nox86exec
  allow-whitelist-disable = 1
  ```

  **No `force-usb-*`, no `hpm*`.** The restore-essential args (`rd=md0`,
  `-restore`, `-progress`) survive only because **iBEC injects them itself** from
  hardcoded strings; they never go through the allowlist.

## §3 — The bypass is security-mode, and it's owner-gated (proven + confirmed)

The allow/deny decision `is_disallowed(config,key)` at `0x81850`:

```
ldrb w8,[config,#8]    ; config[8] = allow-whitelist-disable flag
cmp  w8,#1
b.ne enforce           ; != 1 → enforce allowlist
ldrb w8,[config,#9]    ; config[9] = runtime security byte
tbnz w8,#0, ALLOW_ALL  ; bit0 set → bypass the whole allowlist
enforce: ...comma-match against allowed-boot-args, drop if absent...
```

Config built in `0x81c50`: `config[8]` = `/chosen` `allow-whitelist-disable`
(**= 1 in the restore DT** — first condition already satisfied); `config[9]` is
derived (`0x884b8`) from the **runtime security-state global**, populated at boot
from SEP/hardware — the same source iBEC uses to write `effective-security-mode-ap`
(`0xb7e48`) / `effective-production-status-ap` (`0xb76f8`) into `/chosen`
(builder `0x16780`).

`config[9]` bit0 reflects **security MODE (Full/Reduced/Permissive)** — the
owner-settable LocalPolicy value, **not** the production fuse. Independently
confirmed by Apple's own tooling: `bputil` has a dedicated flag
**`-a` / `--disable-boot-args-restriction`**, documented as *"allows unrestricted
kernel boot arguments via the boot-args nvram variable"* — i.e. exactly this
allowlist filter. (Corroborated by third-party writeups describing the "sip3" bit:
when set, "iBoot won't enforce its built-in allow list for the boot-args nvram
variable.") The disassembly and Apple's flag describe the same switch.

So a retail M1 **can** satisfy the bypass — by being in Permissive Security.

## §4 — The test: RESULT — the ramdisk ignores the installed permissive policy

Set the M1 to Permissive Security from One True Recovery (`sudo bputil -nkca`,
confirmed applied), then re-ran `restore --boot-args force-usb-fullspeed`:

```
DFU Mode        spd=2  high-480M
Recovery Mode   spd=2  high-480M
MacBook Pro     spd=2  high-480M   ← restore ramdisk, permissive set, STILL 480
```

`force-usb-fullspeed` was **still stripped**. So the **DFU-restore ramdisk does
not honor the installed OS's permissive LocalPolicy.**

Why this is the expected outcome: the DFU restore boots a **from-scratch,
Apple-signed restore chain** (iBSS → iBEC → RestoreRamDisk), personalized per
restore via a fresh APTicket. The installed LocalPolicy on the internal SSD
governs booting the **installed OS** — the restore ramdisk is a different boot
object with its **own (production) security state**, so `config[9]` bit0 stays 0
and the allowlist stays enforced (§3). Nothing settable on the installed OS
changes the ramdisk's policy.

**Conclusion: the boot-arg route to USB-3 is closed for DFU restores on the M1**,
even with permissive security — the one arg that would enable it is filtered by a
policy the restore ramdisk enforces independently of the OS. A genuine
"4-minute M1 restore" therefore did **not** come from a DFU restore + boot-arg; it
must have used a different mechanism (e.g. `asr` imaging from a permissive-booted
macOS / target-disk mode, where the OS boot-args apply — not a firmware restore).

Not yet ruled out (long shot): a **revive** (non-obliterate) instead of erase, in
case the ramdisk *does* read the policy but the erase flow resets it first. Low
probability — the ramdisk boot object is the same either way.

## §4b — How Apple gets USB-3 restore: development-fused hardware

The `config[9]` security byte (§3) is built from `effective-production-status-ap`
/ `effective-security-mode-ap`. For the *installed-OS* boot that traces to the
LocalPolicy (settable). For the *restore-ramdisk* boot it does **not** (proven in
§4) — it comes from the **AP production fuse + the signed restore personalization**.

Apple's internal/factory Macs are **development-fused**: `production-status =
false`, `debug-enabled = true`. That state carries into the **restore ramdisk**,
so `config[9]` bit0 is set there, the boot-arg allowlist is bypassed *in the
restore environment itself*, and `force-usb-superspeed` reaches the kernel →
SuperSpeed restore. (They also drive it via internal tooling / builds, but the
fuse is the enabling factor.)

The production fuse is **burned into retail silicon and unchangeable**. LocalPolicy
permissive (what `bputil` sets) only affects the installed OS, never the DFU
restore ramdisk. So a retail M1 can *never* reach USB-3 via the DFU restore path —
regardless of security mode, boot-args, or tooling. A genuine 4-minute M1 restore
was either dev-fused hardware or a non-DFU (`asr` imaging) flow.

### Proven: the gate is a hardware fuse register (not any software input)

Traced `config[9]`'s security byte to its source. The production-status /
security-mode readers (`0xb76f8`→`0x2850c`→`0x28490`, `0xb7e48`) build a fixed
**MMIO address** and read it:

```
mov  x19,#0xc600 ; movk x19,#0x3d2b,lsl#16 ; movk x19,#2,lsl#32  ; x19 = 0x2_3d2b_c600
bl   0xa45d4     ; helper does: ldr w8,[x0]   ← raw load of the register
                 ; compares vs fuse magics 0xa55ac33c / 0xa050c030
```

`0x2_3d2b_c600` is the AP security/fuse block; `0xa45d4` is a bare `ldr w8,[x0]`.
So the value gating the boot-arg allowlist bypass is **read live from a fuse
register**, reflecting the burned production status of retail silicon. **No
software-reachable input — boot-args, device tree, nvram, personalization, or
restore-protocol message — can change what that register reports.** You cannot
write a fuse from software; that is the point of a fuse. This is the immovable
root cause, proven at the register level.

## §5 — Doing it without logging into the Mac

**Not possible on M1.** The permissive bit lives in the LocalPolicy, which is
**signed by the SEP with a key protected by the owner's password**; downgrading
requires authenticating as an owner from recoveryOS. There is no supported no-auth
path, and M1 has no checkm8-class bootROM exploit to bypass the SEP.

Implication for refurbishing *unknown/locked* Macs:
- A DFU restore itself needs no auth, but always installs **Full Security** →
  allowlist enforced → no USB-3.
- To reach permissive you must first own the Mac (which, for an unknown unit, means
  a full — slow — restore + Setup Assistant to establish ownership), then set
  permissive. So the fast path only helps Macs you **already own and have pre-set
  permissive** — it doesn't help a one-shot restore of an incoming unit.
- No restorekit-side change fixes this: the ramdisk's security state comes from the
  Apple-signed personalization / installed LocalPolicy, neither of which we can set
  to permissive without the owner key.

If §4 shows the ramdisk *does* honor permissive, the realistic use is a fleet you
control: pre-set permissive once, then every re-image is fast.

---

## Dead ends (ruled out — don't re-run)

**Thunderbolt restore — not buildable.** The restore DT/kernelcache ship the full
ACIO/Thunderbolt stack (`acio0-7`, `AppleThunderboltIP`, `acio-fw-v1.543`) and the
restore protocol is IP-capable (`RestoreModeIPv6`). But the **ACIO coprocessor
never starts in restore** — measured: target never joined the host TB fabric
(`SPThunderboltDataType` = "No device connected", `AppleThunderboltIP` 0 instances)
until the OS booted. It's `user-power-managed` and the minimal restore env never
requests it. `acio_bringup` is a debug bitmask downstream of the controller
starting (defaults 0 even in the booted OS). Host side is unreachable too: the
TB/USB4 data transport (Apple Config I/O, ACIO/CIO) is gated behind a Transport
Restriction Manager and the same account-attested "authorized transport" wall as
restore attestation; there is no public IOKit TB data-transport API,
`MobileDevice.framework` uses USB-NCM only, and `disable-transport-rm` had no
effect. (This absorbs the former `docs/thunderbolt-restore.md`.)

**`maximum-link-speed` DT property — it's PCIe, not USB.** A flat string-scan
matched `maximum-link-speed` on the `apcie/pci-bridge0` node and a per-board survey
showed 2/3/4 tracking model age — which *looked* like a USB-restore story. A proper
tree-parse shows **no `usb-drd` node on any of 56 boards has `maximum-link-speed`**;
the 2/3/4 is just PCIe generation. USB restore speed is not DT-capped.

**Image-upload chunk size — wrong bottleneck.** The `.dmg.aea` images are
**Stored** (uncompressed) in the IPSW and extract at **347 MB/s** — not a
bottleneck. The ~30 MB/s "uploading image" phase is the restored `FileData`/device
path itself, so 8 KiB→128 KiB chunks didn't move it. Not host-optimizable — the
experimental "larger chunks" idevicerestore patch was reverted as a proven no-op.

**Earlier HPM/`hpmAllowUsb3Restore` "two-gate" analysis — superseded.** The
`AppleTCController::genericStart` / `createUSB3PortObject` disassembly (a
`+0xf84` bit4 "SS link present" gate) is real, but its conclusion ("SS never
trains") was drawn from experiments where the boot-arg was silently stripped (§2),
so it never tested anything. The actual blocker is the allowlist, not link
training. If §4 opens the door, re-evaluate this layer with an arg that truly
reaches the kernel.

---

## Reproduction notes

- **USB-2 ceiling:** during restore the device enumerates
  `usb-drd0-port-hs@… enumerated 0x05ac/12ac (MacBook Pro) at 480 Mbps`,
  `Device Speed = 2` in `ioreg` every run. Apple's own `cfgutil` restores over USB
  too.
- **Monitoring:** use `ioreg -a -r -c IOUSBHostDevice` and parse `Device Speed`
  (2=USB2/480M, 3=USB3/5G, 4=USB3.1/10G) — `system_profiler SPUSBDataType` returns
  nothing in the sandbox. Watch kernel `enumerated … at N Mbps/Gbps` lines via
  `log stream`.
- **Boot-args passthrough:** an idevicerestore patch appended a `--boot-args`
  value to the restore's `setenv boot-args`, exposed as a hidden CLI flag.
  Verified reaching the device but stripped by iBoot's allowlist (§2, §3), so it
  could never take effect on retail silicon — **both the patch and the flag were
  removed** after this conclusion. (Reproducing the tests would require
  re-adding them.)
- **Firmware analysis:** `blacktop/ipsw` (`ipsw img4 im4p extract`,
  `ipsw macho disass`), `nm`, and capstone (flat-image arm64 for iBEC). Targets:
  extracted j293 restore kernelcache, `DeviceTree.j293ap` payload, and
  `iBEC.j293.RELEASE` (LZFSE, not encrypted).
- **Phase timing** (`restorekit restore` reports it): FS upload ~4 min (USB-2 line
  rate), image upload ~3.4 min active (device-bound), device-side install ~2.5 min.
  ~11 min total floor at USB-2.

---

## References

### Apple Silicon boot security, LocalPolicy & permissive mode
- Apple Platform Security — *Boot process for a Mac with Apple silicon*:
  <https://support.apple.com/guide/security/boot-process-secac71d5623/web>
- Apple Platform Security — *Startup Disk security policy control for a Mac with
  Apple silicon* (Full / Reduced / Permissive; permissive not settable from the
  GUI): <https://support.apple.com/guide/security/startup-disk-security-policy-control-sec7d92dc49f/web>
- Apple Platform Security — *Contents of a LocalPolicy file for a Mac with Apple
  silicon* (LocalPolicy is SEP-signed; owner key gated by the owner password):
  <https://support.apple.com/guide/security/contents-a-localpolicy-file-mac-apple-silicon-secc745a0845/web>
- Apple Platform Security — *LLB and iBoot / secure boot chain*:
  <https://support.apple.com/guide/security/secac71d5623/web>
- Howard Oakley (Eclectic Light) — *Mastering Secure Boot on Apple silicon*
  (`bputil`, the three boot-arg gates, the "sip3" boot-args-allowlist bit):
  <https://eclecticlight.co/2024/09/09/mastering-secure-boot-on-apple-silicon/>
- Howard Oakley — *Booting macOS on Apple silicon: LocalPolicy* (Ownership / Owner
  Identity Key, owner-auth requirement to re-sign policy):
  <https://eclecticlight.co/2022/11/21/booting-macos-on-apple-silicon-localpolicy/>
- Howard Oakley — *M1 Secure Boot, morphine and self-destruction* (security modes,
  demotion): <https://eclecticlight.co/2021/05/21/m1-secure-boot-morphine-and-self-destruction/>
- Full Metal Mac — *Secure Boot and Apple Silicon Security*:
  <https://fullmetalmac.com/cybersecurity/macos-security/secure-boot-silicon/>

### bputil / boot-args filtering
- `bputil(1)` man page (`-n` permissive, `-k` kexts, `-c` CTRR, **`-a`
  `--disable-boot-args-restriction`**):
  <https://keith.github.io/xcode-man-pages/bputil.1.html>
- Apple Community — `csrutil disable` from recoveryOS behavior/requirements:
  <https://discussions.apple.com/thread/253397576>

### DFU restore / revive
- Apple — *How to revive or restore Mac firmware* (DFU needs no auth; installs
  Full Security): <https://support.apple.com/en-us/108900>
- Mr. Macintosh — *Restore macOS Firmware on an Apple Silicon Mac + Boot to DFU*:
  <https://mrmacintosh.com/restore-macos-firmware-on-an-apple-silicon-mac-boot-to-dfu-mode/>
- Marc Littlemore — *Rescuing My Bricked M1 MacBook Pro* (DFU restore walkthrough):
  <https://www.marclittlemore.com/rescuing-my-bricked-m1-macbook-pro/>

### USB / DWC3 / device-role speed
- Apple Platform Security — *Directly connecting to another device* / USB
  restrictions context (USB restricted mode background):
  <https://support.apple.com/guide/security/welcome/web>
- Synopsys DesignWare Cores USB 3.x (DWC3) `DCFG.DEVSPD` field encoding — vendor
  databook (0=HS, 4=SS); referenced for the `DCFG.DEVSPD[2:0]` write in §1.

### Tooling used in this investigation
- blacktop `ipsw` (IMG4 / kernelcache / DeviceTree extraction & disassembly):
  <https://github.com/blacktop/ipsw>
- Capstone disassembly engine (flat-image arm64 for iBEC):
  <https://www.capstone-engine.org/>
- libimobiledevice `idevicerestore` (the vendored restore engine restorekit
  patches): <https://github.com/libimobiledevice/idevicerestore>
