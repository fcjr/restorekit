import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { chmod, mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { prepareAppRun, toolsDirectory } from "./prepare-appimage.mjs";

const bytes = Buffer.from("test AppRun binary");
const digest = createHash("sha256").update(bytes).digest("hex");

async function fixture(t, arch = "x86_64") {
  const toolsDir = await mkdtemp(join(tmpdir(), "restorekit-apprun-"));
  t.after(() => rm(toolsDir, { recursive: true, force: true }));
  return {
    platform: "linux", arch, toolsDir,
    hashes: { x86_64: digest, aarch64: digest },
    fetchBinary: async () => bytes,
  };
}

test("matches Tauri's absolute XDG cache directory and HOME fallback", () => {
  assert.equal(toolsDirectory({ XDG_CACHE_HOME: "/tmp/cache" }, "/home/test"), "/tmp/cache/tauri");
  assert.equal(toolsDirectory({ XDG_CACHE_HOME: "relative" }, "/home/test"), "/home/test/.cache/tauri");
  assert.equal(toolsDirectory({}, "/home/test"), "/home/test/.cache/tauri");
});

for (const arch of ["x86_64", "aarch64"]) {
  test(`prepares a clean ${arch} cache with verified contents and mode 0755`, async (t) => {
    const options = await fixture(t, arch);
    let requested;
    options.fetchBinary = async (url) => { requested = url; return bytes; };
    const path = await prepareAppRun(options);
    assert.equal(requested, `https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-${arch}`);
    assert.deepEqual(await readFile(path), bytes);
    assert.equal((await stat(path)).mode & 0o777, 0o755);
    assert.deepEqual(await readdir(options.toolsDir), [`AppRun-${arch}`]);
  });
}

test("repairs cached 0770 without downloading or changing the launcher bytes", async (t) => {
  const options = await fixture(t);
  const path = join(options.toolsDir, "AppRun-x86_64");
  await writeFile(path, bytes);
  await chmod(path, 0o770);
  options.fetchBinary = () => assert.fail("valid cached AppRun must not be downloaded");
  await prepareAppRun(options);
  await prepareAppRun(options); // Repeated bundles stay safe.
  assert.equal((await stat(path)).mode & 0o777, 0o755);
  assert.deepEqual(await readFile(path), bytes);
});

test("rejects a corrupt download without leaving a cache entry", async (t) => {
  const options = await fixture(t);
  options.fetchBinary = async () => Buffer.from("bad download");
  await assert.rejects(prepareAppRun(options), /checksum mismatch/);
  assert.deepEqual(await readdir(options.toolsDir), []);
});

test("rejects a stale cache without changing its contents or permissions", async (t) => {
  const options = await fixture(t);
  const path = join(options.toolsDir, "AppRun-x86_64");
  await writeFile(path, "stale");
  await chmod(path, 0o770);
  await assert.rejects(prepareAppRun(options), /checksum mismatch/);
  assert.equal(await readFile(path, "utf8"), "stale");
  assert.equal((await stat(path)).mode & 0o777, 0o770);
});

test("propagates download failures without leaving a cache entry", async (t) => {
  const options = await fixture(t);
  options.fetchBinary = async () => { throw new Error("offline"); };
  await assert.rejects(prepareAppRun(options), /offline/);
  assert.deepEqual(await readdir(options.toolsDir), []);
});

test("does nothing on macOS and Windows, and rejects unknown Linux architectures", async (t) => {
  const options = await fixture(t);
  options.fetchBinary = () => assert.fail("must not download");
  for (const platform of ["darwin", "windows"]) {
    await prepareAppRun({ ...options, platform, arch: undefined });
  }
  await assert.rejects(prepareAppRun({ ...options, arch: "unknown" }), /Unsupported/);
  assert.deepEqual(await readdir(options.toolsDir), []);
});
