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

## Updating a patch

1. Apply the existing patches to the submodule:
   `cd crates/restorekit-sys/vendor/idevicerestore && git apply ../../patches/idevicerestore/*.patch`
2. Make your edits, then regenerate: `git diff src/ > ../../patches/idevicerestore/NNNN-name.patch`
   (split by hand if regenerating a single patch of several).
3. Revert the submodule: `git checkout -- src/` — the working tree must stay
   clean so `git submodule status` stays pristine.

After bumping a submodule pin, re-verify every patch still applies
(`cargo build -p restorekit-sys` fails loudly if one doesn't).
