# applerestore — Implementation Plan

Working checklist; check items off as they land.

## 1. Scaffold
- [x] Workspace `Cargo.toml` (crates/applerestore, crates/applerestore-cli), MIT LICENSE, NOTICE (Apache-2.0 attribution for the vdm port), .gitignore
- [x] docs/prd.md
- [x] docs/plan.md (this file)
- [x] README.md (install, usage, DFU port locations, safety warning)

## 2. Library core
- [x] `error.rs` — thiserror `Error` enum covering discovery, resolution, download, checksum, restore, host-support failures
- [x] `progress.rs` — serializable `Event` enum; all long operations take `&mut dyn FnMut(Event)`

## 3. DFU discovery (cross-platform)
- [x] `dfu/discovery.rs` — nusb enumeration, VID 0x05ac / PID 0x1227, DFU serial-string parser (`CPID:... BDID:... ECID:...`)
- [x] `wait_for_dfu(timeout)` polling loop for post-trigger / manual DFU entry
- [x] `device.rs` — static (CPID, BDID) → (board, identifier, marketing name) table generated from ipsw.me `/v4/devices`; runtime ipsw.me fallback for unknown boards
- [x] Unit tests: serial parser, table lookup

## 4. Firmware resolve + cache + download
- [x] ipsw.me `/v4/device/{identifier}?type=ipsw` resolver (latest signed, or pinned `--os-version`)
- [x] mesu.apple.com `com_apple_macOSIPSW.xml` fallback resolver (latest only)
- [x] Cache dir resolution: `${XDG_CONFIG_HOME:-~/.config}/applerestore/firmwares` (+ env/flag overrides)
- [x] Resumable download (`Range` into `.partial`, atomic rename), SHA-256/SHA-1 verification, cache hit short-circuit
- [x] Unit tests: resolver parsing (JSON + plist fixtures), cache-dir resolution

## 5. DFU trigger (macOS Apple Silicon only)
- [x] `dfu/vdm.rs` — Rust port of macvdmtool: AppleHPM IOKit plug-in FFI (COM vtable), LOCK unlock w/ platform-name key, Gaid reset retry, DBMa enter/exit (RAII), VDMs send + reg 0x4d ack polling
- [x] `enter_dfu()` (VDM `{0x5ac8012, 0x106, 0x80010000}`) and `reboot()` (VDM `{0x5ac8012, 0x105, 0x80000000}`)
- [x] Root + Apple Silicon host guards with clear errors; manual DFU instructions helper for unsupported hosts
- [x] **Hardware-verified**: an Apple Silicon host triggered DFU on a target Mac, which was then detected and identified correctly

## 6. Restore engine (statically-linked libidevicerestore, FFI-only)
Self-contained binary: the idevicerestore C stack is built from pinned sources
and linked in. No subprocess, no `brew install idevicerestore`.
- [x] `applerestore-sys` crate: git submodules (`vendor/`) pinning libplist, libimobiledevice-glue, libusbmuxd, libirecovery, libtatsu, libimobiledevice, idevicerestore
- [x] `build.rs` builds the stack in cargo flow: openssl/zlib/curl via vendored `-sys` crates, libzip via CMake, the 6 autotools libs into a staging prefix, then compiles idevicerestore's `.c` sources (with `main` renamed) and emits static link directives + macOS frameworks
- [x] FFI decls (`idevicerestore_client_new/set_flags/set_ipsw/set_ecid/set_progress_callback/start/get_error`, `FLAG_ERASE`)
- [x] `restore.rs` rewritten over the FFI (progress callback → `Event::RestoreStep`), subprocess path removed
- [x] Native macOS build links cleanly (self-contained: no Homebrew dylib deps — verify with `otool -L`)
- [x] Unit tests: progress step-name mapping

## 7. CLI
- [x] clap derive skeleton: `status`, `dfu`, `reboot`, `download`, `restore`, `run`, `cache`; global `--cache-dir`, `--json`, `-v`
- [x] indicatif progress rendering from library events; JSON-lines mode
- [x] Erase confirmation prompt (model + ECID, `--yes` to skip)

## 8. CI/CD (native per-platform builds — the static C stack rules out zig cross-compile)
- [x] `.github/workflows/ci.yml` — fmt, clippy, test; matrix macos-14 (arm64) + ubuntu (needs `libusb-1.0`, autotools, cmake to build the stack)
- [x] Native release build matrix: macos-14 (arm64), macos-13 (x86_64), ubuntu-24 arm64 + x86_64 — each runs `cargo build --release` (build.rs builds the self-contained stack), uploads the artifact
- [x] `.goreleaser.yaml` — `builder: prebuilt` assembling the per-platform artifacts into archives + checksums; `homebrew_casks` → fcjr/homebrew-fcjr with quarantine-removal hook
- [x] `.github/workflows/release.yml` — tag-triggered: build matrix → goreleaser packages prebuilt binaries; `GITHUB_TOKEN` + `TAP_GITHUB_TOKEN`

## 9. Verification
- [x] `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`
- [x] `applerestore status` / `download --identifier ... ` smoke tests against live APIs
- [ ] `goreleaser check` (goreleaser not installed locally — validated in CI)
- [x] Release build (`cargo build --release`) links; 8.2 MB self-contained binary, no third-party dylibs
- [x] Hardware: `sudo applerestore dfu` + `status` verified against the cabled target Mac (detection + model ID confirmed)
- [ ] (Manual, destructive — needs explicit go-ahead) full `applerestore run` erase restore over the FFI

## Post-v1 follow-ups (not in this pass)
- [ ] Create github.com/fcjr/applerestore and push; add `TAP_GITHUB_TOKEN` secret (PAT with write access to fcjr/homebrew-fcjr)
- [ ] Code signing + notarization for the macOS binaries
- [ ] Tauri desktop UI over the library
- [ ] Windows packaging/testing
