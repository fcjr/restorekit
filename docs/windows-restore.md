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
   the store (iPhones unaffected): **force** our plain `winusb.sys` onto that one
   device instance with `UpdateDriverForPlugAndPlayDevices(..., INSTALLFLAG_FORCE)`
   — verified live, after which libusb opens it and the restore proceeds.

### Done since

- **Restore-mode force-bind is automated.** At restore start restorekit spawns an
  elevated watcher (one UAC, branded restorekit, skipped when already admin) that
  force-binds our WinUSB to the restore-mode device when it appears — per-restore,
  since Apple's driver reclaims it each time. `appleusb.inf` stays in the store.
- **The watcher stages the driver package itself.** With Apple's own driver
  installed (iTunes, Apple Devices, or Windows Update), DFU and recovery work
  without `setup-driver`, so a restore could reach restore mode with no
  RestoreKit package in the store; the watcher then had nothing to bind and the
  restore stalled at "waiting for device to enter restore mode" (#56). The
  elevated watcher now stages the package before it starts watching, logs what
  it did to `%TEMP%\restorekit-restore-watch.log`, and the restore stops up
  front if the watcher can't start (UAC declined) instead of stalling later.
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
- **Certificate trust.** `Import-Certificate` into
  `LocalMachine\TrustedPublisher` is refused on current Windows 11 even when
  elevated; the setup script uses `X509Store` instead.
- **Throughput confirmed.** With the 0ms-while-connected event loop, the CLI
  streams `Cryptex1,systemOS` at ~35 MB/s (near USB-2.0 line rate) — the embedded
  usbmuxd + WinUSB bulk path is not a bottleneck.

- **Driverless with Apple's stack.** Windows Update installs Apple's drivers
  for DFU (`WUDFRd`), recovery and KIS (`AppleKIS`) on first plug-in, and
  libirecovery's native Windows backend talks to them directly. When Apple
  Mobile Device Service (iTunes or Apple Devices) is running, it holds
  `127.0.0.1:27015` and muxes the restore-mode interface through Apple's
  driver, so restorekit uses it instead of starting the embedded usbmuxd and the
  WinUSB watcher. It's recognized by matching the port's listener pid against the
  service's pid from the service manager; any other listener on the port fails
  the restore up front with its name. restorekit reads a KIS Mac's identity
  through AppleKIS's GET_INFO ioctl and counts Apple's drivers as ready.
  Hardware-tested: a full revive of an M4 mac mini (KIS DFU via a dongle) on
  Windows 11 ARM64 with no RestoreKit driver package, no UAC prompt and no test
  signing.
- **AppleKIS sometimes misses the Mac.** Entering KIS DFU from a fully booted
  Mac, AppleKIS leaves its interface disabled about half the time (no
  `KIS\VID_05AC&PID_0001` child), and the Mac stays unreadable until it
  re-enters DFU; restarting the device doesn't help. With a dongle, restorekit
  now reboots the Mac out of DFU and triggers again (up to twice) when it lands
  unreadable. Measured on the M4 mini: every unreadable entry read fine after
  one re-entry.
- **Windows on ARM without Apple's service fails up front.** Our self-signed
  package can't be staged there (see Known issues), so a restore with no Apple
  Mobile Device Service and no staged package stops before uploading anything
  and says to install Apple Devices or iTunes; `setup-driver` failures on ARM
  say the same.

### Known issues

- **Windows on ARM.** Code Integrity rejects our self-signed catalog there
  ("Driver package signer is not trusted by system, and Code Integrity is
  enforced"), so `setup-driver` can't stage the package unless test signing is
  on (`bcdedit /set testsigning on`). With Apple Mobile Device Service installed
  no package is needed (see above), but a dongle on firmware 0.1.2 or older
  still needs `setup-driver` for its interface GUID; update it from macOS or
  Linux.
- **Classic DFU (`05ac:1227`) through Apple's driver is untested here.** The
  rig's M4 only enters KIS DFU over the dongle. #56's log shows Port DFU, DFU
  and recovery going through libirecovery on Windows, but not which driver was
  bound.

- **Desktop app throughput.** The same restore that runs at ~35 MB/s from the CLI
  is much slower in the desktop app (the muxer gets fed at only tens of
  packets/sec). The C/USB stack is identical, so the throttle is somewhere in the
  desktop-specific integration, not usbmux/libusb. Restores still complete, so
  this is a performance issue, not a correctness one. Not yet root-caused.
