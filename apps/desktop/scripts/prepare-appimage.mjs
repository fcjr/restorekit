#!/usr/bin/env node
// Tauri CLI 2.11.4 downloads AppRun with mode 0770. linuxdeploy preserves it as
// AppRun.wrapped, then appimagetool records root ownership: ordinary users cannot
// launch the mounted image. Prepare the same binary with 0755 BEFORE bundling
// and updater signing. Remove this workaround when the locked CLI fixes it.
import { createHash } from "node:crypto";
import { chmod, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// The exact apprun-old assets used by the locked Tauri bundler.
const appRuns = {
  x86_64: "f30140a43a0a59e46db21bdefdf749b9e9f2c6946e92afabbacf98b8ae73fb4f",
  aarch64: "072f17c0895a85c490282fe5395c5007e5fc75da727e553b3b8fb680feb11578",
};

export function toolsDirectory(env = process.env, home = homedir()) {
  // Match dirs::cache_dir(), used by Tauri with useLocalToolsDir=false.
  const cache = env.XDG_CACHE_HOME;
  return join(cache && isAbsolute(cache) ? cache : join(home, ".cache"), "tauri");
}

async function download(url) {
  const response = await fetch(url, { signal: AbortSignal.timeout(60_000) });
  if (!response.ok) throw new Error(`AppRun download failed: HTTP ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}

export async function prepareAppRun({
  platform = process.env.TAURI_ENV_PLATFORM ?? process.platform,
  arch = process.env.TAURI_ENV_ARCH,
  toolsDir = toolsDirectory(),
  fetchBinary = download,
  hashes = appRuns,
} = {}) {
  if (platform !== "linux") return;
  const expected = hashes[arch];
  if (!expected) throw new Error(`Unsupported AppImage architecture: ${arch}`);
  const path = join(toolsDir, `AppRun-${arch}`);
  let bytes;
  let missing = false;
  try {
    bytes = await readFile(path);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    missing = true;
    bytes = await fetchBinary(
      `https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-${arch}`,
    );
  }
  if (createHash("sha256").update(bytes).digest("hex") !== expected) {
    throw new Error(`AppRun checksum mismatch: ${path}. Remove a stale cache file and retry.`);
  }
  if (missing) {
    await mkdir(toolsDir, { recursive: true });
    const temporary = `${path}.${process.pid}.tmp`;
    try {
      await writeFile(temporary, bytes, { flag: "wx", mode: 0o600 });
      await chmod(temporary, 0o755);
      await rename(temporary, path);
    } finally {
      await rm(temporary, { force: true });
    }
  } else {
    await chmod(path, 0o755);
  }
  return path;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    // This hook runs from apps/desktop. Guard against silently preparing the
    // wrong cache if the project's bundler configuration changes in future.
    const config = JSON.parse(await readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url)));
    if (process.env.TAURI_ENV_PLATFORM === "linux" && config.bundle.useLocalToolsDir) {
      throw new Error("Update prepare-appimage.mjs for useLocalToolsDir before bundling");
    }
    const path = await prepareAppRun();
    if (path) console.log(`Prepared executable AppImage launcher: ${path}`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
