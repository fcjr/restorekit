# Building restorekit from source

Most people should **not** need this. The prebuilt binaries —
`brew install fcjr/fcjr/restorekit-cli`, `scoop install restorekit-cli`, the
`--cask` desktop app, and the [release downloads](https://github.com/fcjr/restorekit/releases) —
are self-contained and need nothing installed. Build from source only if you're
contributing, packaging for an unsupported platform, or want the latest `main`.

## Why source builds need a toolchain

`restorekit-cli` links [`restorekit-sys`](../crates/restorekit-sys), which builds
the whole idevicerestore C stack from vendored sources at compile time — libplist,
libimobiledevice-glue, libusbmuxd, libirecovery, libtatsu, libimobiledevice, and
idevicerestore via autotools, libzip via CMake, plus OpenSSL/curl/zlib through
vendored `-sys` crates. So `cargo install` / `cargo build` need **autotools, CMake,
pkg-config, a C compiler, and libusb** present first. On Windows this specifically
means an MSYS2 + GNU-toolchain environment — the default MSVC toolchain can't build
an autotools stack.

Everywhere you also need a [Rust toolchain](https://rustup.rs) (stable).

---

## macOS

```sh
xcode-select --install   # C compiler, make, etc.
brew install autoconf automake libtool pkg-config cmake autoconf-archive
```

Then:

```sh
cargo install restorekit-cli
```

## Linux

### Debian / Ubuntu

```sh
sudo apt-get update
sudo apt-get install -y \
  build-essential autoconf automake libtool pkg-config cmake autoconf-archive \
  libusb-1.0-0-dev libssl-dev libcurl4-openssl-dev zlib1g-dev
```

### Other distributions

Install the equivalents of: a C compiler + `make`, `autoconf`, `automake`,
`libtool`, `autoconf-archive`, `pkgconf`/`pkg-config`, `cmake`, and the dev
headers for `libusb-1.0`, OpenSSL, curl, and zlib. For example:

```sh
# Fedora
sudo dnf install gcc make autoconf automake libtool autoconf-archive pkgconf \
  cmake libusb1-devel openssl-devel libcurl-devel zlib-devel

# Arch
sudo pacman -S base-devel autoconf automake libtool autoconf-archive pkgconf \
  cmake libusb openssl curl zlib
```

Then:

```sh
cargo install restorekit-cli
```

## Windows

The vendored C stack is autotools-based, so it builds under **MSYS2** with the
**GNU** Rust toolchain — not the default `x86_64-pc-windows-msvc`. A plain
`cargo install restorekit-cli` on stock Windows will fail; follow these steps.

1. **Install [MSYS2](https://www.msys2.org/)** and open the **MINGW64** shell
   (the blue icon — *not* the MSYS or UCRT64 shell; the toolchain differs).

2. **Install the build tools** in that shell:

   ```sh
   pacman -S --needed base-devel git make autoconf automake libtool \
     autoconf-archive gettext-devel pkgconf perl \
     mingw-w64-x86_64-gcc mingw-w64-x86_64-pkgconf mingw-w64-x86_64-cmake \
     mingw-w64-x86_64-libusb mingw-w64-x86_64-nasm
   ```

3. **Add the GNU Rust toolchain** (from PowerShell, or in the MINGW64 shell if
   `rustup` is on your `PATH`):

   ```sh
   rustup toolchain install stable-x86_64-pc-windows-gnu
   ```

4. **Build from the MINGW64 shell** so `gcc` and the autotools are on `PATH`. If
   `cargo` isn't visible there, add your rustup bin dir first:

   ```sh
   export PATH="$USERPROFILE/.cargo/bin:$PATH"
   cargo +stable-x86_64-pc-windows-gnu install restorekit-cli \
     --target x86_64-pc-windows-gnu
   ```

   (Or run `rustup default stable-x86_64-pc-windows-gnu` once and drop the
   `+…`/`--target` flags.)

---

## Building the repository (contributors)

A git checkout keeps the C sources as submodules, so init them before building:

```sh
git clone https://github.com/fcjr/restorekit
cd restorekit
git submodule update --init --recursive
cargo build --release
```

(The published crate on crates.io already ships the vendored sources, so
`cargo install restorekit-cli` doesn't need the submodule step.)

The first build compiles the entire C stack and takes a few minutes; subsequent
builds are cached. The desktop app under [`apps/desktop`](../apps/desktop) has
its own prerequisites (Node, the Tauri CLI, and WebKitGTK on Linux) — see the
[Tauri prerequisites](https://tauri.app/start/prerequisites/).
