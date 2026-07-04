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
- **Throughput confirmed.** With the 0ms-while-connected event loop, the CLI
  streams `Cryptex1,systemOS` at ~35 MB/s (near USB-2.0 line rate) — the embedded
  usbmuxd + WinUSB bulk path is not a bottleneck.

### Known issues

- **Desktop app throughput.** The same restore that runs at ~35 MB/s from the CLI
  is much slower in the desktop app (the muxer gets fed at only tens of
  packets/sec). The C/USB stack is identical, so the throttle is somewhere in the
  desktop-specific integration, not usbmux/libusb. Restores still complete, so
  this is a performance issue, not a correctness one. Not yet root-caused.
