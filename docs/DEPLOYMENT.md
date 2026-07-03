# Deployment

How `restorekit` is built, released, and distributed. Cutting a release is a
single tag push; everything below happens automatically in GitHub Actions.

## Distribution channels

| Channel | What ships | Who it's for |
| --- | --- | --- |
| **Homebrew** (`fcjr/homebrew-fcjr`) | Prebuilt binaries, as a cask | End users: `brew install fcjr/fcjr/restorekit-cli` (the `restorekit` cask token is reserved for the desktop app) |
| **GitHub Releases** | `tar.gz` archives per platform + checksums | Direct downloads, scripts |
| **crates.io** | Source crates (`restorekit-sys`, `restorekit`, `restorekit-cli`) | `cargo install restorekit-cli`, and Rust consumers of the library |

## Cutting a release

1. Bump the version in the root `Cargo.toml` (`[workspace.package] version`) and
   the internal dependency versions in `crates/restorekit/Cargo.toml` and
   `crates/restorekit-cli/Cargo.toml` (they pin `version = "x.y.z"`).
2. Commit, then tag and push:

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

The `release` workflow (`.github/workflows/release.yml`) then runs three things
in parallel:

- **`build`** — compiles a self-contained binary natively on four runners
  (macOS arm64/x64, Linux arm64/x64) and uploads each as an artifact.
- **`release`** — downloads those artifacts and runs GoReleaser to publish the
  GitHub Release (archives + checksums) and push the updated Homebrew cask.
- **`crates`** — publishes the three crates to crates.io in dependency order.

## Required secrets

Set these in the `fcjr/restorekit` repo under **Settings → Secrets and
variables → Actions**:

| Secret | Used for | Scope |
| --- | --- | --- |
| `GORELEASER_GITHUB_TOKEN` | Pushing the Homebrew cask + Scoop bucket (CLI via goreleaser, app cask via `release-app.yml`) | PAT with **Contents: write** on `fcjr/homebrew-fcjr` (and the Scoop bucket repo) |
| `CARGO_REGISTRY_TOKEN` | Publishing to crates.io | A crates.io API token with publish scope |

The built-in `GITHUB_TOKEN` handles the GitHub Release itself (granted
`contents: write` in the workflow) — no setup needed.

## Desktop app (RestoreKit.app)

`release-app.yml` builds, **signs, and notarizes** the macOS app with
`tauri-apps/tauri-action` on the same `v*` tags. The privileged DFU helper is
built and staged as a Tauri sidecar first (`apps/desktop/stage-helper.sh`).

Signing + notarization secrets (Developer ID required — the app uses a signed
privileged helper for the DFU trigger):

Signing (Developer ID):

| Secret | What it is |
| --- | --- |
| `APPLE_CERTIFICATE` | The Developer ID Application cert + key exported as a base64 `.p12` (export from Keychain Access, then `base64 -i cert.p12 \| pbcopy`) |
| `APPLE_CERTIFICATE_PASSWORD` | The password you set on that `.p12` |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: <Org> (<TEAMID>)` |

Notarization (App Store Connect API key — from App Store Connect → Users and
Access → Integrations → keys):

| Secret | What it is |
| --- | --- |
| `APPLE_API_ISSUER` | The Issuer ID (UUID) |
| `APPLE_API_KEY` | The Key ID (10 chars, e.g. `Y6269F7S2X`) |
| `APPLE_API_KEY_P8_BASE64` | The `.p8` file **base64-encoded**: `base64 -i AuthKey_<KEYID>.p8 \| pbcopy`. Base64 (single line) is masked reliably in logs; the raw multi-line PEM is not. The workflow decodes it into `$RUNNER_TEMP` (auto-wiped). |

GitHub Actions auto-masks any registered secret in logs (`***`). The workflow
passes the key through `env:` — never inlined into a command — and stores it
base64 so masking is robust; nothing prints the key.

To sign + notarize a build locally: point the env at the `.p8` directly and run
`npm run tauri build` in `apps/desktop`:

```sh
export APPLE_SIGNING_IDENTITY="<sha1 of your Developer ID cert>"   # a hash disambiguates duplicate certs
export APPLE_API_ISSUER="<issuer-uuid>"
export APPLE_API_KEY="<key-id>"
export APPLE_API_KEY_PATH="$HOME/Downloads/AuthKey_<key-id>.p8"
npm run tauri build
```

Omit the three `APPLE_API_*` vars to sign without notarizing.

### App cask

On each `v*` tag, `release-app.yml` fills `apps/desktop/homebrew/restorekit.rb`
(the cask template) with the release version and the `.dmg`'s sha256 and pushes
it to `fcjr/homebrew-fcjr` as `Casks/restorekit.rb` (using `TAP_GITHUB_TOKEN`),
so `brew install --cask restorekit` tracks the latest notarized build. Edit the
template in this repo to change cask metadata — the workflow only substitutes the
version and sha256. The cask is arm64-only and requires macOS 13+ (SMAppService).

## How the crates.io publish works

The three crates publish in dependency order (`restorekit-sys` →
`restorekit` → `restorekit-cli`); `cargo publish` waits for each to appear
in the index before the next. Publishing uses `--no-verify`: `ci.yml` is the
build gate, and verifying here would rebuild the entire C stack three times.

`restorekit-sys` vendors its C sources (libimobiledevice stack, libzip,
idevicerestore) as git submodules, but `cargo package` includes them in the
published `.crate` (~2.2 MiB compressed), so it builds on a consumer's machine
with **no submodules required** — only the C toolchain (see below).

## Build prerequisites (for `cargo install` / from source)

The binary statically links its C stack, so building needs a C toolchain:

- **macOS:** Xcode command-line tools. Nothing else — it links the system
  IOKit/Security frameworks.
- **Linux:**

  ```sh
  sudo apt-get install -y \
    build-essential autoconf automake libtool pkg-config cmake autoconf-archive \
    libusb-1.0-0-dev libssl-dev libcurl4-openssl-dev zlib1g-dev
  ```

  The `-dev` packages only satisfy the vendored libraries' `configure` checks;
  OpenSSL, libcurl, and zlib are still linked statically. The resulting binary
  depends only on `libc` and `libusb` (verified with `ldd`).

## Verifying locally before a release

- `cargo fmt --all --check && cargo clippy --workspace -- -D warnings && cargo test --workspace`
- Linux build in a container (arm64):
  `docker run --rm -i -v "$PWD":/src:ro ubuntu:24.04 bash -s < <build script>`
  (mirrors the CI apt list; see git history for the exact script).
- `cargo package --list -p restorekit-sys` to confirm the vendored sources are
  included in the crate.

## First-time setup checklist

- [ ] Create the `fcjr/restorekit` GitHub repo and push.
- [ ] Create the `TAP_GITHUB_TOKEN` and `CARGO_REGISTRY_TOKEN` secrets.
- [ ] Confirm the `fcjr/homebrew-fcjr` tap repo exists.
- [ ] Reserve the crate names on crates.io by publishing `v0.1.0` (names are
      first-come; the tag push handles it).
