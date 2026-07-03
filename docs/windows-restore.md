# Windows restore — status & remaining work

Windows uses the same engine as Linux (Windows == Linux feature set: detect,
download, restore; no electronic DFU trigger). This tracks how far a real restore
gets on Windows and what's left.

## Verified working on Windows (Apple Silicon Mac, hardware-tested)

A restore runs end-to-end **through the recovery→restore-mode handoff**:

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

## The remaining wall: restore-mode / usbmux

The restore hangs at `Waiting for device to enter restore mode`. The device *is*
in restore mode (PID 0x12ac) with **WinUSB bound via Apple's `appleusb.inf`**
(a composite driver — plus a USB-NCM interface). idevicerestore reaches restore
mode over the **usbmux / `restored`** protocol, which needs a **usbmuxd**.

**Embedded usbmuxd is now ported to Windows** (compiles + links): the vendored
daemon builds on MinGW behind a small POSIX→Winsock shim (`csrc/win_shim/`: fake
`sys/socket.h`/`poll.h`/`netinet/tcp.h` with a BSD `tcphdr`/`sys/time.h`/`syslog.h`
plus `close`→`closesocket`, fcntl-nonblock→`ioctlsocket`, sockopt casts,
`localtime_r`), and `usbmuxd_server.c` listens on TCP `127.0.0.1:27015`
(libusbmuxd's Windows default) with `WSAPoll`. The `UsbmuxdGuard` starts it on
Windows too. We **play nice with `appleusb.inf`** — it binds the WinUSB interface,
so no driver removal / WHQL-signing war.

### Where it now stands (hardware-tested, restore-mode)

The restore reaches restore mode and the embedded usbmuxd runs, but two
**libusb-on-Windows** limitations block the final device comms:

1. `libusb_get_pollfds()` is **not implemented on libusb's Windows backend**
   ("external polling of libusb's internal event sources is not yet supported on
   Windows"). usbmuxd integrates libusb into its `poll()` loop via that call, so
   on Windows it falls back to timeout-only polling (noisy, degraded). Not
   necessarily fatal, but the event model differs.
2. **The hard blocker:** libusb's WinUSB backend can't open the restore-mode
   device — `winusbx_open ... PID_12AC&RESTORE_MODE&MI_00 (interface 0): [50] The
   request is not supported` → `LIBUSB_ERROR_IO`. Interface topology (device live
   in restore mode):
   - `MI_00` — **WinUSB**, bound by **Apple's `appleusb.inf`** (status OK)
   - `MI_01`/`MI_02` — UsbNcm / NCM data (both `Error`)
   Crucially, `MI_00` *is* the WinUSB interface — but it's Apple's WinUSB variant,
   which libusb rejects. DFU and recovery worked because `setup-driver` bound plain
   `winusb.sys` to those PIDs; the restore-mode device (0x12ac) is claimed by
   Apple's driver instead.

### Remaining work (concrete)

- **Bind our WinUSB to the restore-mode interface.** Extend `setup-driver`'s INF
  to match `USB\VID_05AC&PID_12xx&RESTORE_MODE&MI_00` (safe — the `RESTORE_MODE`
  qualifier is Mac-restore-only, so it won't hijack normal-mode iPhones/iPads at
  0x129x). A more-specific, trusted match should win over `appleusb.inf`, giving
  libusb a plain WinUSB interface it can open — exactly as DFU/recovery already do.
- Address libusb's missing `libusb_get_pollfds` on Windows (usbmuxd's event loop
  falls back to timeout polling — noisy but appeared non-fatal during the
  recovery-mode uploads; adapt or patch/upstream if it blocks the muxer).
- The `MI_01`/`MI_02` NCM interfaces are in `Error`; may or may not matter once
  the WinUSB path opens.

Everything up to this point (DFU → recovery → all component uploads → entering
restore mode, with usbmuxd ported) is committed and works on real hardware.
