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
- `0012-fail-the-restore-when-a-handler-fails.patch` — when a message
  handler fails (an iPhone's Yonkers TSS request rejected, say), the restore
  loop logged "Unable to successfully restore device" and set `FLAG_QUIT`,
  but still read and handled one more message. That handler's result
  overwrote `err`, so `restore_device()` returned 0, idevicerestore printed
  `DONE`, and restorekit reported a restore that left the device stuck in
  restore mode as complete. Break out of the loop with the error instead.
- `0013-route-plist-dumps-through-the-print-callback.patch` —
  `logger_dump_plist()` ignored the print callback: with the log file set to
  `NONE` (as `restorekit_install_log_capture` does), it wrote request,
  response and device-info plists straight to stdout. They bypassed the
  capture, cluttered non-verbose runs, and, stdout being block-buffered when
  piped, came out after the restore's final status line. Serialize the plist
  and hand it to the callback like any other message.
- `0014-report-device-operations-and-the-filesystem-stream.patch` — the
  progress callback only got idevicerestore's coarse steps, and the device's
  progress messages for one step (the firmware updater, say) keep arriving
  while the filesystem streams for another, so a timing breakdown built on them
  put a four-minute upload down to the firmware updater. Report each operation
  the device starts (`RESTORE_STEP_OPERATION` plus its code, named by
  `restore_progress_string()`), and the ASR stream as its own step,
  `RESTORE_STEP_SEND_FS`, from 0 when it starts to 1 once the last chunk is out.

## libimobiledevice-glue

- `0001-real-condition-variables-on-windows.patch` — on Windows `cond_t` was a
  semaphore: a signal with no waiter was counted instead of dropped, so a later
  wait returned at once, and a wait never re-took the mutex, so callers then
  released it twice. idevicerestore's device waits are written for pthreads, so
  one stray device event made the next "wait for the device to disconnect"
  return immediately and fail the restore. Use a `CONDITION_VARIABLE` with the
  `CRITICAL_SECTION` mutex, which behaves like pthreads. Built with autotools
  from a patched copy, like libirecovery.

## libimobiledevice

- `0001-answer-a-pairing-challenge-with-the-challenged-record.patch` — with no
  pair record passed in, every `Pair` request generates a new one: a new
  HostID and new root and host keys. Pairing with a supervised device takes two
  requests, the second answering the challenge the first got, so the answer
  went out with a record the device had never seen. Keep the record a request
  was challenged with on the lockdownd client and send that one with the next
  `Pair`, as pymobiledevice3 does.

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
- `0006-debug-usb-links-over-winusb.patch` — with libusb, libirecovery opens
  each KIS device through one link per process, shared by its clients and its
  device event thread, and runs recovery mode over RSM on it, which a MacBook
  Neo's iBoot needs: it has no USB device of its own. On Windows with WinUSB
  bound to `05ac:1881` (0002), run the same links on the WinUSB pipes, so a
  restore finds recovery mode there too, and the device event thread watches
  WinUSB-bound KIS devices the same way. A failed transfer resets its pipe;
  only removal marks a link dead. Devices bound to Apple's AppleKIS driver are
  unchanged.
- `0007-ask-before-opening-a-kis-device.patch` — a KIS device's ECID can only
  be read by opening it and sending it requests, so each restore's device event
  thread and `irecv_open_with_ecid()` did that to every Mac in DFU over Debug
  USB, including ones other processes were restoring. On macOS, where
  `deviceinterfaced` lets several processes reach one device, those requests
  landed between another restore's transfers, failed (or broke them), and a
  Mac whose own device read failed was never announced, so its restore gave up
  with "Unable to discover device mode". `irecv_set_kis_filter()` takes a
  callback asked with each KIS device's IOKit `locationID` before it is opened;
  one it turns down is passed over unopened. restorekit's turns down a Mac
  another process's restore holds (`restorekit::kis::claim`).

## libusbmuxd

- `0001-set-socket-address-per-thread.patch` — libusbmuxd finds usbmuxd from
  `USBMUXD_SOCKET_ADDRESS`, which applies to the whole process. On Linux a
  restore points it at the embedded usbmuxd, which only sees restore mode, so
  `recovery::enter` running alongside couldn't reach a running iPhone or iPad
  through the system usbmuxd. Add `libusbmuxd_set_thread_socket_address`, which
  sets the address for the calling thread ahead of the variable (and without
  reading it, so a concurrent `set_var` can't race the lookup).

## usbmuxd

- `0001-use-the-active-configuration.patch` — on finding an Apple device,
  usbmuxd searched every configuration for a mux interface, detached the
  kernel drivers and switched the device to that configuration. Embedded in a
  restore tool, that reconfigures every other Apple device on the host as soon
  as the server starts. A Mac in restore mode already has its mux interface in
  the active configuration, so only look there, matching class, subclass and
  protocol together, and leave a device without one alone, logging that once
  per device so a stall at restore mode says why. Linux can activate
  a new configuration at the same address without a hotplug event, so the
  periodic discovery pass stays on next to hotplug there.
- `0002-switch-running-devices-on-request.patch` — with the first patch the
  server never moves a running iPhone or iPad, which boots into a
  configuration without its mux interface, so `recovery::enter` needed the
  system usbmuxd on Linux. Add `usb_set_switch_running`, off by default: while
  on, a configured device in the iPhone/iPad PID range (not a Mac) without an
  active mux is opened, its kernel drivers unbound and its mux configuration
  set, as upstream usbmuxd does. `recovery::enter` turns it on only while it
  runs.

## Updating a patch

1. Apply the existing patches to the submodule:
   `cd crates/restorekit-sys/vendor/idevicerestore && git apply ../../patches/idevicerestore/*.patch`
2. Make your edits, then regenerate: `git diff src/ > ../../patches/idevicerestore/NNNN-name.patch`
   (split by hand if regenerating a single patch of several).
3. Revert the submodule: `git checkout -- src/` — the working tree must stay
   clean so `git submodule status` stays pristine.

After bumping a submodule pin, re-verify every patch still applies
(`cargo build -p restorekit-sys` fails loudly if one doesn't).
