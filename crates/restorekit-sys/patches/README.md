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

## Updating a patch

1. Apply the existing patches to the submodule:
   `cd crates/restorekit-sys/vendor/idevicerestore && git apply ../../patches/idevicerestore/*.patch`
2. Make your edits, then regenerate: `git diff src/ > ../../patches/idevicerestore/NNNN-name.patch`
   (split by hand if regenerating a single patch of several).
3. Revert the submodule: `git checkout -- src/` — the working tree must stay
   clean so `git submodule status` stays pristine.

After bumping a submodule pin, re-verify every patch still applies
(`cargo build -p restorekit-sys` fails loudly if one doesn't).
