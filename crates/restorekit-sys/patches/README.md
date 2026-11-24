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
  components (cryptexes, recoveryOS) stream as tens of thousands of plist
  chunks with no recovery; one dropped USB write aborted the whole
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
- `0009-wait-longer-for-ibec-to-boot.patch` — after iBEC is sent from DFU,
  `dfu_enter_recovery` gave the Mac 10 seconds to drop off the bus and another
  10 to come back in recovery. An M4 mac mini whose earlier restore was cut
  short took about 30 seconds to boot iBEC, so every retry failed with "Device
  did not disconnect. Possibly invalid iBEC". Wait up to 60 seconds each, like
  the same step in `idevicerestore.c` already does.
- `0010-send-file-data-in-128k-chunks.patch` — component `FileData` went out
  in 8 KiB plist chunks, each a separate message through usbmuxd. Send
  128 KiB, the chunk size ASR already uses for the filesystem. On Windows a
  6.5 GB Rosetta cryptex took 250 seconds in 8 KiB chunks and about 150 in
  128 KiB ones.
- `0011-update-only-from-an-upgrade-install-identity.patch` — without
  `FLAG_ERASE`, idevicerestore looked for the device's Upgrade Install
  identity and, when the image had none, fell back to the model's first
  identity, which can be an Erase Install. A revive could then erase the
  device. Fail with "Unable to find a matching build identity" instead.

## libimobiledevice-glue

- `0001-real-condition-variables-on-windows.patch` — on Windows `cond_t` was a
  semaphore: a signal with no waiter was counted instead of dropped, so a later
  wait returned at once, and a wait never re-took the mutex, so callers then
  released it twice. idevicerestore's device waits are written for pthreads, so
  one stray device event made the next "wait for the device to disconnect"
  return immediately and fail the restore. Use a `CONDITION_VARIABLE` with the
  `CRITICAL_SECTION` mutex, which behaves like pthreads. Built with autotools
  from a patched copy, like libirecovery.

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
- `0003-match-kis-interface-to-enumeration.patch` — `dic_find_interface` took
  a lone KIS interface as the device's own when none matched its registry entry
  id. With more than one Mac attached that isn't safe: the daemon can list one
  Mac's interface while another is being opened, and the request would go to
  the wrong Mac. Only use an interface that matches the device.
- `0004-hold-the-device-while-a-hotplug-thread-adds-it.patch` — the libusb
  hotplug callback handed its `libusb_device` to a detached thread without a
  reference, though libusb only guarantees it during the callback. With
  several Macs re-enumerating at once, the thread could read a freed device and
  crash in `libusb_get_device_descriptor`. Reference the device for the thread
  and release it when the thread is done.
- `0005-open-the-kis-device-with-the-requested-ecid.patch` — a KIS device has
  no serial, so `irecv_open_with_ecid()` can only learn its ECID by opening
  it. Every backend returned the first KIS device it could open, and the
  caller then rejected it as the wrong device, so with two Macs in DFU over
  Debug USB only the first could be restored. Try each KIS device in turn
  until one answers with the requested ECID, passing over one that can't be
  claimed or initialised (another restore's Mac).

## usbmuxd

- `0001-use-the-active-configuration.patch` — on finding an Apple device,
  usbmuxd searched every configuration for a mux interface, detached the
  kernel drivers and switched the device to that configuration. Embedded in a
  restore tool, that reconfigures every other Apple device on the host as soon
  as the server starts. A Mac in restore mode already has its mux interface in
  the active configuration, so only look there, matching class, subclass and
  protocol together, and leave a device without one alone. Linux can activate
  a new configuration at the same address without a hotplug event, so the
  periodic discovery pass stays on next to hotplug there.

## Updating a patch

1. Apply the existing patches to the submodule:
   `cd crates/restorekit-sys/vendor/idevicerestore && git apply ../../patches/idevicerestore/*.patch`
2. Make your edits, then regenerate: `git diff src/ > ../../patches/idevicerestore/NNNN-name.patch`
   (split by hand if regenerating a single patch of several).
3. Revert the submodule: `git checkout -- src/` — the working tree must stay
   clean so `git submodule status` stays pristine.

After bumping a submodule pin, re-verify every patch still applies
(`cargo build -p restorekit-sys` fails loudly if one doesn't).
