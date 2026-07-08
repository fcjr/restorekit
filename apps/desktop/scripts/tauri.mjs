#!/usr/bin/env node
// Wrapper so `pnpm tauri ...` works from any shell on Windows without setting
// environment variables by hand.
//
// The Rust side links the idevicerestore C stack and `windows-sys` with the GNU
// toolchain (x86_64-pc-windows-gnu), which needs MinGW's binutils (notably
// `dlltool.exe`) and the MSYS2 tools on PATH. Rather than require every
// contributor to prepend those dirs, add them here before invoking Tauri.
// No-op on macOS/Linux, and only adds directories that actually exist.
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";

if (process.platform === "win32") {
  const root = process.env.MSYS2_ROOT || "C:\\msys64";
  const dirs = [`${root}\\mingw64\\bin`, `${root}\\usr\\bin`].filter(existsSync);
  const current = process.env.PATH ?? "";
  const lower = current.toLowerCase();
  const missing = dirs.filter((d) => !lower.includes(d.toLowerCase()));
  if (missing.length) {
    process.env.PATH = `${missing.join(";")};${current}`;
  }
}

// A single command string (rather than an args array) with `shell: true` both
// resolves the `tauri` shim on Windows (.cmd) and avoids Node's DEP0190 warning.
// The args come from our own `pnpm tauri …`, so concatenation is safe here.
const command = ["tauri", ...process.argv.slice(2)].join(" ");
const child = spawn(command, { stdio: "inherit", shell: true, env: process.env });
child.on("exit", (code) => process.exit(code ?? 1));
