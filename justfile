# Show available recipes
default:
    @just --list

# Install dev dependencies: JS workspace + the firmware's Rust toolchain
install:
    pnpm install
    rustup target add thumbv6m-none-eabi
    rustup component add llvm-tools
    cargo install flip-link elf2uf2-rs cargo-binutils

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

# Build the RP2040 dongle firmware + bootloader (prereqs: just install)
fw-build:
    cd crates/dongle-lite-fw && cargo build --release
    cd crates/dongle-lite-boot && cargo build --release

# Update a dongle's firmware over USB (production path: no bootrom, no drive)
fw-update: fw-build
    cd crates/dongle-lite-fw && cargo objcopy --release -- -O binary --remove-section=.boot2 target/dongle-lite-fw.bin
    cargo run -q -p restorekit-cli -- dongle update crates/dongle-lite-fw/target/dongle-lite-fw.bin

# Flash bootloader + app over the RP2040 bootrom (factory / first flash)
fw-flash-full: fw-build
    #!/usr/bin/env bash
    set -euo pipefail
    cargo run -q -p restorekit-cli -- dongle bootsel || true
    elf2uf2-rs crates/dongle-lite-boot/target/thumbv6m-none-eabi/release/dongle-lite-boot \
               crates/dongle-lite-boot/target/dongle-lite-boot.uf2
    elf2uf2-rs crates/dongle-lite-fw/target/thumbv6m-none-eabi/release/dongle-lite-fw \
               crates/dongle-lite-fw/target/dongle-lite-fw.uf2
    python3 scripts/merge-uf2.py \
        crates/dongle-lite-boot/target/dongle-lite-boot.uf2 \
        crates/dongle-lite-fw/target/dongle-lite-fw.uf2 \
        crates/dongle-lite-fw/target/dongle-lite-full.uf2
    mount=$({{wait_rpi_rp2}})
    cp crates/dongle-lite-fw/target/dongle-lite-full.uf2 "$mount/"

# Flash the app over the RP2040 bootrom (first time: use fw-flash-full)
fw-flash: fw-build
    #!/usr/bin/env bash
    set -euo pipefail
    # If a dongle is up, drop it into the bootloader over its vendor USB
    # interface — no button needed. A fresh Pico (no firmware yet) must be
    # plugged in with BOOTSEL held instead, so a missing dongle isn't an error.
    cargo run -q -p restorekit-cli -- dongle bootsel || true
    ( {{wait_rpi_rp2}} ) > /dev/null
    cd crates/dongle-lite-fw
    elf2uf2-rs -d target/thumbv6m-none-eabi/release/dongle-lite-fw

# Shell snippet: wait for the RP2040 bootrom drive, print its mount point.
wait_rpi_rp2 := '''
    for _ in $(seq 1 30); do
        for m in /Volumes/RPI-RP2 "/run/media/$USER/RPI-RP2" "/media/$USER/RPI-RP2"; do
            if [ -d "$m" ]; then echo "$m"; exit 0; fi
        done
        sleep 0.5
    done
    echo "RPI-RP2 drive never appeared; is the board in BOOTSEL?" >&2
    exit 1
'''
