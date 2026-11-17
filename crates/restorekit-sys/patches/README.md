# Vendored-source patches

Patches applied on top of the pinned submodules at build time. The submodules
themselves stay pristine: `build.rs` copies the sources into `OUT_DIR`, applies
each project's `*.patch` files there in filename order (`git apply`), and
compiles the patched copy.

## idevicerestore

- `0001-send-file-data-as-binary-plist.patch` — send component `FileData`
  chunks as binary plists instead of XML on data-port connections, cutting the
  ~35% base64 overhead (the same format the URLAsset response already uses).
- `0002-retry-component-sends-on-transport-failure.patch` — multi-gigabyte
  components (cryptexes, recoveryOS) stream as hundreds of thousands of 8K
  plist chunks with no recovery; one dropped USB write aborted the whole
  restore. Reconnect to the data port and re-send the component from the
  start, up to 3 attempts.
- `0006-fix-double-free-in-dfu-open-retry.patch` — libirecovery's
  `irecv_open_with_ecid()` frees the client on its error paths without clearing
  `*pclient`, so `irecv_open_with_ecid_and_attempts()` frees the same pointer
  again on its next iteration. Apple Silicon Macs hit it reliably when they
  re-enumerate from DFU into recovery mode after iBSS. Drive the retry loop
  from `dfu.c`, clearing the pointer after each failed attempt.
- `0007-keep-waiting-for-device-after-restore-mode-reboot.patch` — when a
  restore starts with the target still in restore mode (a retry after the
  link dropped mid-restore), idevicerestore reboots it and waits up to 60s
  for it to come back. That wait was a single `cond_wait_timeout`, which any
  add/remove event for the device ends early, including the device dropping
  off USB as it reboots or re-enumerating once it's back. It then bailed with
  "Unable to discover device mode" while the Mac was still on its way back.
  Loop until the mode is known or the minute is up.
- `0008-send-appletcon-firmware-without-response-tags.patch` — since upstream
  started honoring `ResponseTags`, the AppleTCON updater only gets
  `FirmwareData` when the request lists it. A Mac16,5 sends its request with
  no `ResponseTags` at all, gets just the Baobab ticket, and fails checkpoint
  0x1332 ("failed to copy response for AppleTCON") while the restore carries
  on. When `ResponseTags` is missing, send the firmware like before, falling
  back to the ticket-only reply if it can't be extracted.

## libirecovery

- `0001-kis-via-deviceinterfaced-on-macos.patch` — macOS only lets processes
  holding `com.apple.private.debug-usb.access` open a Debug USB (KIS,
  `05ac:1881`) device, and only Apple's `deviceinterfaced` has it. On macOS,
  open KIS devices through the daemon's private `DeviceInterfaceClient`
  framework and send KIS requests with its `transfer_data`, instead of failing
  to open the device (or dropping its device-add event). Loaded at runtime;
  `IRECV_KIS_NO_DIC=1` forces the plain IOKit path. Unlike idevicerestore,
  libirecovery is built with autotools from a patched copy of the whole tree,
  rebuilt only when the patches change.
- `0002-kis-over-winusb-on-windows.patch` — libirecovery's Windows KIS code
  talks to Apple's AppleKIS driver, which does the KIS framing itself behind
  its own ioctls. restorekit binds plain WinUSB to `05ac:1881` instead, so a
  KIS device was never found. When the KIS interface turns out to be a WinUSB
  device, speak raw KIS on its bulk pipes through `WinUsb_*` (the same requests
  the libusb and IOKit backends send). The SetupAPI device checks (open, add
  and the hotplug poller) accept pid `1881` next to AppleKIS's pid 1, so
  idevicerestore gets the device-add event it reads the mode from. The AppleKIS
  path is untouched.

## Updating a patch

1. Apply the existing patches to the submodule:
   `cd crates/restorekit-sys/vendor/idevicerestore && git apply ../../patches/idevicerestore/*.patch`
2. Make your edits, then regenerate: `git diff src/ > ../../patches/idevicerestore/NNNN-name.patch`
   (split by hand if regenerating a single patch of several).
3. Revert the submodule: `git checkout -- src/` — the working tree must stay
   clean so `git submodule status` stays pristine.

After bumping a submodule pin, re-verify every patch still applies
(`cargo build -p restorekit-sys` fails loudly if one doesn't).
