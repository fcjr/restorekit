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
mode over the **usbmux / `restored`** protocol, which needs a **usbmuxd**, and the
embedded usbmuxd server is currently **Linux-only** (`#[cfg(target_os = "linux")]`
in `crates/restorekit/src/usbmuxd.rs` and `compile_usbmuxd` in
`crates/restorekit-sys/build.rs`).

Good news: we **play nice with `appleusb.inf`** — it provides the WinUSB
interface we need, so no driver removal or WHQL-signing war. The one piece is the
usbmux bridge.

### Port plan (embedded usbmuxd → Windows)

Vendored `usbmuxd` daemon is POSIX-oriented but the socket surface is small
(`client.c` 6 refs, `device.c`/`utils.c` a couple, `usb.c` is portable libusb).

1. **`win_shim/` headers** — provide `sys/socket.h`, `sys/un.h`, `unistd.h`,
   `poll.h` (and `netinet/in.h`, `arpa/inet.h`) that pull in `<winsock2.h>` /
   `<ws2tcpip.h>`. Put this dir first on the include path so only the *missing*
   POSIX headers resolve to it; existing ones (`sys/stat.h`…) fall through to
   MinGW.
2. **Compat mapping**: `close`→`closesocket` (usbmuxd uses `fclose` for files, so
   socket-only), `poll`→`WSAPoll`, non-blocking via `ioctlsocket(FIONBIO)`,
   `errno`→`WSAGetLastError` where it matters, `SOCKET` vs `int` fd handling.
3. **`usbmuxd_server.c` shim**: `#ifdef _WIN32` path — `WSAStartup`, listen on
   **TCP 127.0.0.1:27015** instead of an AF_UNIX socket, `WSAPoll` instead of
   `ppoll` (drop `sigset`).
4. **`usbmuxd.rs`**: Windows guard variant — no Unix socket path; rely on
   libusbmuxd's default TCP 27015 (or set `USBMUXD_SOCKET_ADDRESS=127.0.0.1:27015`).
5. **`build.rs`**: enable `compile_usbmuxd` on Windows (drop `HAVE_PPOLL`/
   `HAVE_CLOCK_GETTIME`/`HAVE_LOCALTIME_R`; add `win_shim` include; link `ws2_32`).
6. Then debug the usbmux protocol over TCP at runtime against a real restore.

Also watch: the restore-mode device's **USB-NCM interface showed `Error`** —
Apple Silicon restore does a USB-network connection, so that path may need
attention once usbmux connects.
