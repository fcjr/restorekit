# Faster Apple Silicon restore: Thunderbolt vs USB3 investigation

Goal: make the DFU restore faster than the USB-2 (480 Mbps) ceiling that
dominates the ASR filesystem upload. This documents everything learned chasing
that, so nobody re-runs the dead ends — and so we can keep pushing the one lead
that's still open (USB3 "SuperSpeed" restore).

Hardware under test: **M1 MacBook Pro** (CPID `0x8103`, board j293-class),
host-based DFU from an Apple Silicon Mac, OWC Thunderbolt 4 cable.

## TL;DR

- **USB-2 (480 Mbps) is the observed ceiling** for DFU restore on this M1 — every
  tool, every run. Confirmed at the enumeration level (`usb-drd0-port-hs … at
  480 Mbps`), and Apple's own `cfgutil` restores over USB too.
- **Thunderbolt restore is a dead end** on this platform: the device keeps its
  Thunderbolt controller (ACIO coprocessor) powered off during restore, there's
  no host userspace TB data API, and even Apple doesn't use it. Details below.
- **USB3 "SuperSpeed" restore is a real, restore-aware mechanism** — there's a
  `hpmAllowUsb3Restore` / `-hpmForceAllowUSB3` boot-arg path, and the M1's
  restore port has a SuperSpeed sub-port (`usb-drd0-port-ss`). But setting those
  args did **not** engage USB3 on the M1 (still 480 Mbps). Why is unresolved.
- The open question is decidable only with the **target's** restore-mode kernel
  log, which the restore protocol doesn't relay to the host.

## The USB-2 ceiling (confirmed)

During restore mode the device enumerates as:

```
usb-drd0-port-hs@00100000: enumerated 0x05ac/12ac (MacBook Pro) at 480 Mbps
```

`usb-drd0` is the Mac's built-in USB **device-role** controller; `-port-hs` is
its high-speed (USB-2) sub-port. `Device Speed = 2` (USB-2) in `ioreg` throughout
every restore. This is why the ASR upload is slow regardless of tooling.

(`system_profiler SPUSBDataType` returns nothing in our sandbox — use `ioreg -a
-r -c IOUSBHostDevice` and parse `Device Speed`: 2=USB2/480M, 3=USB3/5G,
4=USB3.1/10G. `SPThunderboltDataType` does work.)

## Thunderbolt: why it's a dead end

The pieces exist on the device, but the path is blocked:

- The restore **device tree** contains the full ACIO/Thunderbolt hierarchy
  (`acio0-7`, `acio-cpu0-7`, `thunderbolt-drom`, ATC DisplayPort nodes), and the
  restore **kernelcache** ships the drivers + firmware (`AppleThunderboltIP`,
  `AppleThunderboltNHIGenericACIO`, `acio-fw-v1.543`). The restore protocol is
  even IP-based (`RestoreModeIPv6`, `restored` listens on sockets).
- **But the Thunderbolt controller never starts in restore.** Measured directly:
  across a full restore the target never joined the host's TB fabric
  (`SPThunderboltDataType` = "No device connected") and `AppleThunderboltIP` had
  0 instances — then Thunderbolt came up the instant the OS booted. The ACIO
  controller is an on-demand (`user-power-managed`) coprocessor and the minimal
  restore environment (no `configd`, only a USB-NCM control channel) never
  requests it.
- `acio_bringup` is a real boot-arg but is a **debug bitmask** read *inside* the
  NHI driver's power code — downstream of the controller starting. It defaults 0
  in the booted OS too (where TB works fine), so it isn't the gate; setting it
  does nothing when the controller never powers on.
- The host side is unreachable regardless: macOS exposes **no public IOKit
  Thunderbolt data-transport API**, and `MobileDevice.framework` reaches restore
  devices over USB-NCM only (no TB path). `disable-transport-rm` had no effect.

Net: Thunderbolt restore would need Apple to start the ACIO coprocessor in the
restore environment (it doesn't) *and* an entitled host transport (doesn't exist
publicly). Not buildable.

## USB3 SuperSpeed: the live lead

This is the promising one, and it matches the "super speed" idea.

- **The hardware supports it:** the M1 device-role restore port has a SuperSpeed
  sub-port — `usb-drd0-port-ss` sits right next to `usb-drd0-port-hs` in the
  device tree (`usb-drd,t8103`).
- **There's a restore-aware software mechanism.** In `AppleTCController::
  genericStart` (in `com.apple.driver.AppleHPM`):
  - `PE_parse_boot_argn("hpmAllowUsb3Restore", &buf, 1)` → logs
    `***allow usb3 restores: %u***`. Confirmed a real boot-arg.
  - The **"In Restore"** branch of the same function explicitly reads
    `-hpmForceAllowUSB3`, sets a state byte, and logs `Force allow USB3`.
  - USB3 port objects exist: `createUSB3PortObject` / `configureUSB3PortObject`.

So the intent is clearly there: a Mac in restore *can* be told to allow/force a
USB3 link.

### What we tried, and the result

Via restorekit's `--boot-args` passthrough (patch 0005), two obliterate runs:

| boot-args | restore-mode speed |
|---|---|
| `hpmAllowUsb3Restore=1` | USB-2 (480 Mbps) |
| `hpmAllowUsb3Restore=1 -hpmForceAllowUSB3` | USB-2 (480 Mbps) |

Both stayed USB-2. `hpmAllowUsb3Restore` is confirmed reachable by our args (it's
a real `PE_parse_boot_argn`), so the mechanism *received* the flag — but the port
never re-enumerated onto `-port-ss`.

### Controller selection — resolved (the arg *was* processed)

Which Type-C controller drives the port is chosen by IOKit probe against
device-tree properties:

- `AppleTCControllerSingleTransport::probe` matches **only if `hpm-transport-type`
  is present and non-zero** (else returns NULL).
- `AppleTCController` / `AppleTCControllerType10/12/14/15::probe` match on
  **`hpm-class-type`**.

The target is confirmed **j293ap (MacBook Pro 13" M1**, `BDID 0x24 / CPID 8103`).
Its device tree has **`hpm-class-type` but no `hpm-transport-type`** — so
`SingleTransport` **cannot match**, and the port is driven by an
`AppleTCControllerType1x` variant, all of which **inherit `genericStart`** (the
USB3 path). Therefore `-hpmForceAllowUSB3` was almost certainly **processed** by
`genericStart`'s "In Restore" branch. (My earlier "SingleTransport ⇒ arg is
inert" claim was wrong — corrected here.)

So the failure is downstream of the flag: the SuperSpeed **link never trained**.

### Where it actually stalls: device-role USB3

The Type-C/HPM layer allowing USB3 is necessary but not sufficient. The target is
a USB **device** (peripheral) presenting the restore gadget, and its device-role
SuperSpeed is a separate layer (the DWC3 / `usb-drd` controller). Findings:

- The USB arbitrator has a boot-arg to force USB3 **host** mode
  (`AppleEmbeddedUSBArbitrator-force-usb3host`) but **no `-force-usb3device`
  equivalent** — the device-role side isn't forceable by boot-arg.
- The DWC3 SuperSpeed pipe control (`…GUSB3PIPECTL…`) and `tunable_DRD_USB31_DEV`
  govern device-mode SS, but those tunables are **absent from the j293 device
  tree** — so the DT doesn't hard-cap it, yet SS still didn't come up.
- There's a `force-usb-highspeed` boot-arg (forces USB-2) and
  `UsbHostPortLinkSpeedLimit` — evidence the stack has explicit speed-limiting.
- `USB3DeviceNeedsAuthentication` exists — a possible gate on device-role USB3.

Best current read: the DFU/restore **gadget is brought up as high-speed by the
boot chain**, and while the Type-C layer can be told to *allow* USB3, nothing in
the reachable boot-args re-defines the device-role gadget as SuperSpeed or
triggers a re-enumeration onto `-port-ss`.

### The two-gate confirmation (why the arg is processed yet nothing happens)

Traced the flag `-hpmForceAllowUSB3` sets (`AppleTCController` object `+0xfa9`)
to where it's consumed:
`AppleTCControllerAssignableTransport::createUSB3PortObject`. That function needs
**both** of these, and does a bare `retab` (silent early-return, no log) if
either is missing:

```asm
ldrb w8, [x0, #0xfa9]   ; force-USB3 flag  (set by -hpmForceAllowUSB3)  ✓
tbz  w8, #0, bail
ldrb w8, [x19, #0xf84]  ; port capabilities/status byte
tbz  w8, #4, bail       ; bit 4 = SuperSpeed link present/active         ✗
```

`+0xf84` is a status byte (100+ `ldrb` reads, no `strb` writes — set via a wider
store from port state), **not** a boot flag, so it can't be forced. It's clear
because the restore connection is physically USB-2 (`usb-drd0-port-hs`); the
SuperSpeed lanes never negotiate, so the port is never marked SS-capable, so
`createUSB3PortObject` bails silently.

**Conclusion:** the "allow USB3 restore" knob is real and reachable, but it sits
*behind* a capability bit that reflects an actual SuperSpeed link — and a DFU
restore never establishes one (bootROM gadget = USB-2, no re-enumeration to
`-port-ss`). So USB3 restore is not reachable by boot-arg on the M1 DFU path: it
would require the restore gadget to physically come up / re-enumerate as
SuperSpeed, which nothing in the boot chain or the reachable args triggers. This
matches every measurement (480 Mbps every run) and explains the silent failure.

## Open questions / next levers

1. **Host side.** SuperSpeed needs *both* ends. The host runs full macOS; its
   `usb-drd0` enumerated the target on `-port-hs`. Try `AppleEmbeddedUSBArbitrator
   -force-usb3host` (and/or a SS-capable state) **on the host** (host nvram +
   reboot) so the host offers SS host mode to the target.
2. **`USB3DeviceNeedsAuthentication`.** Understand what authenticates device-role
   USB3 and whether restore satisfies it.
3. **Re-enumeration trigger.** The HS link is established by the bootROM before
   the ramdisk; find what (if anything) would make the device-role gadget
   re-enumerate as SS after `genericStart` allows it.
4. **Target log capture (decisive).** A serial/KDP console into the restore
   ramdisk would show `genericStart - Force allow USB3` and any subsequent
   device-role SS attempt/failure — the one datapoint the host can't otherwise
   see. Not available over the standard restore protocol.

## Reproduction notes

- Boot-args reach the restore ramdisk via `restore_boot_args` → iBoot
  `setenv boot-args` → the ramdisk kernel (restorekit patch 0005 appends
  `--boot-args`).
- Monitor from the host with `ioreg` (not `system_profiler SPUSBDataType`);
  watch `Device Speed` and the `enumerated … at N Mbps/Gbps` kernel log lines.
- Kernelcache/device-tree analysis used `blacktop/ipsw` (`ipsw img4 im4p
  extract`, `ipsw macho disass -t <kext> --vaddr`, `nm`) against the extracted
  restore kernelcache and `DeviceTree.j293ap` payload.
