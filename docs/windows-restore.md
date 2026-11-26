# Windows restore — status & remaining work

Windows uses the same engine as Linux (Windows == Linux feature set: detect,
download, restore; no electronic DFU trigger). This tracks how far a real restore
gets on Windows and what's left.

## Verified working on Windows (Apple Silicon Mac, hardware-tested)

A **full restore completes** end-to-end:

- CLI + desktop app build and link the full idevicerestore C stack on
  `x86_64-pc-windows-gnu` (MSYS2/MinGW, GNU host toolchain).
- **Detect** a cabled Mac in DFU/recovery — driverless (nusb/SetupAPI).
- **WinUSB setup** (`restorekit setup-driver` / the app's "Set up USB access")
  binds WinUSB to the DFU/recovery product ids so libusb can open the device.
- **Download** the IPSW (lock-protected, resumable, Content-Length-verified).
- **Restore**: identifies the device, fetches SHSH blobs from Apple's TSS,
  personalizes and uploads *every* component over WinUSB (iBEC, SEP, kernelcache,
  213 MB ramdisk, device tree…), then the device enters **restore mode**.

Native crashes fixed along the way (all Windows-only, behind `cfg`/`windows`):
stack overflow (restore now on a 64 MB-stack thread), 20 GB IPSW `stat`
(`_FILE_OFFSET_BITS=64`), and idevicerestore's progress mutex used before init.

## Restore mode / usbmux — solved

A **full restore now completes on Windows** (hardware-tested, Apple Silicon):
DFU → recovery → all component uploads → restore mode → Cryptex/SystemOS/
BaseSystem → recovered. The restore-mode phase needed three pieces:

1. **Embedded usbmuxd ported to Windows.** The vendored daemon builds on MinGW
   behind a small POSIX→Winsock shim (`csrc/win_shim/`: fake `sys/socket.h`/
   `poll.h`/`netinet/tcp.h` with a BSD `tcphdr`/`sys/time.h`/`syslog.h` plus
   `close`→`closesocket`, fcntl-nonblock→`ioctlsocket`, sockopt casts,
   `localtime_r`). `usbmuxd_server.c` listens on TCP `127.0.0.1:27015`
   (libusbmuxd's Windows default, so no client config); `UsbmuxdGuard` starts it.
2. **Event loop without `libusb_get_pollfds`.** libusb's Windows backend doesn't
   implement it (USB I/O runs on an internal thread), so usbmuxd's `poll()` loop
   busy-spun and starved transfers. On Windows we skip `usb_get_fds`, poll only
   the listen/client sockets, and pump `usb_process()` each iteration — 0ms while
   a client (a live restore) is connected for full bulk throughput, 10ms idle.
3. **WinUSB on the restore-mode interface.** In restore mode the Mac becomes a
   composite whose `...&RESTORE_MODE&MI_00` interface is bound by Apple's
   `appleusb.inf`, whose WinUSB variant libusb **can't open** (`winusbx_open` →
   `[50] ERROR_NOT_SUPPORTED`). Apple's INF lists that exact hardware id and is
   WHQL-signed, so we can't win by ranking. The fix that keeps `appleusb.inf` in
   the store (iPhones unaffected): install Windows' own `winusb.inf` "WinUsb
   Device" onto that one device instance (see "No driver package" below),
   after which libusb opens it and the restore proceeds.

### Done since

- **Restore-mode force-bind is automated.** At restore start restorekit spawns an
  elevated watcher (one UAC, branded restorekit, skipped when already admin) that
  force-binds our WinUSB to the restore-mode device when it appears — per-restore,
  since Apple's driver reclaims it each time. `appleusb.inf` stays in the store.
- **No driver package, so nothing to sign.** restorekit used to generate a
  WinUSB INF, sign its catalog with a self-signed certificate it added to the
  machine's trusted roots, and stage it with `pnputil`. Windows on ARM refuses
  such packages outright (Code Integrity only loads Microsoft-signed ones), and
  a restore could reach restore mode with no package staged (#56). Now it binds
  Windows' own Microsoft-signed `winusb.inf` instead: `SetupDiBuildDriverInfoList`
  over just that INF with `DI_FLAGSEX_ALLOWEXCLUDEDDRVS` (its "WinUsb Device"
  entry matches `USB\MS_COMP_WINUSB`, not the Mac, which is what Device
  Manager's "Have Disk" route allows too), then `DiInstallDevice` on the one
  device instance, after writing the mode's interface GUID to the device's
  `DeviceInterfaceGUIDs` the way an INF's `AddReg` would. A device already on
  WinUSB without a GUID (a dongle on firmware 0.1.2 or older) just gets the
  GUID and a restart. No certificate, no catalog, no `pnputil`, and it works
  the same on x64 and ARM64. Hardware-tested on Windows 11 ARM64 with test
  signing off, no Apple Mobile Device Service and no RestoreKit package or
  certificate: a full revive of the M4 mini (KIS DFU via the dongle, the
  watcher binding the restore-mode interface) ran to Restore Finished.
- **Device waits on Windows.** libimobiledevice-glue's Windows `cond_t` was a
  semaphore, so a device event nobody was waiting for made the next wait
  return at once, and idevicerestore gave up on the device; it's a real
  condition variable now. `dfu_enter_recovery` also gives iBEC 60 seconds to
  boot instead of 10 (see `crates/restorekit-sys/patches/README.md`).
- **The watcher binds whatever the restore needs.** The elevated watcher (log:
  `%TEMP%\restorekit-restore-watch.log`) binds the restore-mode interface when
  it appears, and any DFU, recovery or KIS device that has had no driver for a
  few seconds (a machine without Apple's drivers), since each mode the Mac
  passes through is a new device. The restore stops up front if the watcher
  can't start (UAC declined) instead of stalling later.
- **`setup-driver` leaves Apple's drivers alone.** Windows Update installs
  Apple's WHQL-signed drivers (`appleusb.inf`, `applekis.inf`) on first
  plug-in, and they work with restorekit (see "Driverless" below). Moving a KIS
  Mac off AppleKIS strands it until it re-enters DFU, so `setup-driver` only
  binds WinUSB to devices on no driver or a non-Apple one, and counts devices on
  Apple's drivers as set up.
- **Dongles work on Windows.** The dongle firmware asked for WinUSB but gave no
  device interface GUID, so Windows never exposed the vendor interface to
  nusb ("Could not find DeviceInterfaceGUIDs in registry"). The next firmware adds
  it; `setup-driver` binds the interface with the GUID for older firmware, and
  runs with only a dongle connected.
- **Throughput confirmed.** With the 0ms-while-connected event loop, the CLI
  streams `Cryptex1,systemOS` at ~35 MB/s (near USB-2.0 line rate) — the embedded
  usbmuxd + WinUSB bulk path is not a bottleneck.

- **Driverless with Apple's stack.** Windows Update installs Apple's drivers
  for DFU (`WUDFRd`), recovery and KIS (`AppleKIS`) on first plug-in, and
  libirecovery's native Windows backend talks to them directly. When Apple's
  usbmuxd is running, it holds
  `127.0.0.1:27015` and muxes the restore-mode interface through Apple's
  driver, so restorekit uses it instead of starting the embedded usbmuxd and the
  WinUSB watcher. RestoreKit recognizes the Apple Mobile Device Service by its
  service PID and the Microsoft Store version of Apple Devices by the mux
  executable in its installed package. Any other listener on the port fails
  the restore up front. restorekit reads a KIS Mac's identity
  through AppleKIS's GET_INFO ioctl and counts Apple's drivers as ready.
  Hardware-tested: a full revive of an M4 mac mini (KIS DFU via a dongle) on
  Windows 11 ARM64 with no RestoreKit driver package, no UAC prompt and no test
  signing.
- **A KIS Mac sometimes comes up unreadable.** Entering KIS DFU from a fully
  booted Mac, about half the time the Mac doesn't answer the driver's first
  requests: AppleKIS leaves its interface disabled (no `KIS\VID_05AC&PID_0001`
  child), and over WinUSB the config write fails. It stays that way until it
  re-enters DFU; restarting the device doesn't help. With a dongle, restorekit
  reboots the Mac out of DFU and triggers again (up to twice) when it lands
  unreadable. Measured on the M4 mini over AppleKIS and WinUSB: every
  unreadable entry read fine after one or two re-entries.

### Known issues

- **Classic DFU (`05ac:1227`) through Apple's driver is untested here.** The
  rig's M4 only enters KIS DFU over the dongle. #56's log shows Port DFU, DFU
  and recovery going through libirecovery on Windows, but not which driver was
  bound.

- **Desktop app throughput.** The same restore that runs at ~35 MB/s from the CLI
  is much slower in the desktop app (the muxer gets fed at only tens of
  packets/sec). The C/USB stack is identical, so the throttle is somewhere in the
  desktop-specific integration, not usbmux/libusb. Restores still complete, so
  this is a performance issue, not a correctness one. Not yet root-caused.
