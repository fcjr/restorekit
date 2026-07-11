# Install dev dependencies: JS workspace + the firmware's Rust toolchain
install:
    pnpm install
    rustup target add thumbv6m-none-eabi
    cargo install flip-link elf2uf2-rs

# On Windows the vendored C stack is built with autotools, which can't target
# MSVC: cargo needs the GNU toolchain, with MinGW's binutils and the MSYS2
# tools on PATH (mirrors apps/desktop/scripts/tauri.mjs and ci.yml).
msys2_root := env("MSYS2_ROOT", "C:/msys64")
win_cargo_env := "PATH=\"" + msys2_root + "/mingw64/bin:" + msys2_root + "/usr/bin:$PATH\""

# Run the CLI (dev build), passing arguments through
[unix]
cli *args:
    cargo run -p restorekit-cli -- {{args}}

# Run the CLI (dev build), passing arguments through
[windows]
cli *args:
    {{win_cargo_env}} cargo run --target x86_64-pc-windows-gnu -p restorekit-cli -- {{args}}

# Build the CLI (release)
[unix]
cli-build:
    cargo build --release -p restorekit-cli

# Build the CLI (release)
[windows]
cli-build:
    {{win_cargo_env}} cargo build --release --target x86_64-pc-windows-gnu -p restorekit-cli

# Run the desktop app with hot reload
app-dev:
    cd apps/desktop && pnpm tauri dev

# Build the desktop app (unsigned; see app-build-signed)
app-build:
    cd apps/desktop && pnpm tauri build

# Build a signed + notarized RestoreKit.app (config from apps/desktop/.env)
app-build-signed:
    cd apps/desktop && pnpm build:signed

# Build the RP2040 dongle firmware (prereqs: just install)
fw-build:
    cd crates/dongle-lite-fw && cargo build --release

# Build the firmware and push it to a dongle over USB
fw-flash: fw-build
    #!/usr/bin/env bash
    set -euo pipefail
    cd crates/dongle-lite-fw
    # If a dongle is up, its `bootsel` console command reboots it into the
    # bootloader — no button needed. A fresh Pico (no firmware yet) must be
    # plugged in with BOOTSEL held instead. Writes to CDC1 are harmless, so
    # don't bother picking the control port out of the pair.
    shopt -s nullglob
    for port in /dev/cu.usbmodem*DPL* /dev/serial/by-id/*Dongle*; do
        printf 'bootsel\r' > "$port" 2>/dev/null || true
    done
    # Wait for the bootloader drive, then deploy (elf2uf2-rs converts the
    # ELF, copies it over, and the board reboots into the new image).
    for _ in $(seq 1 30); do
        if [ -d /Volumes/RPI-RP2 ] || [ -d "/run/media/$USER/RPI-RP2" ] || [ -d "/media/$USER/RPI-RP2" ]; then
            break
        fi
        sleep 0.5
    done
    elf2uf2-rs -d target/thumbv6m-none-eabi/release/dongle-lite-fw
