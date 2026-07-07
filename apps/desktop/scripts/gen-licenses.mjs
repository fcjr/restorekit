// Regenerate the bundled third-party license attribution shown in the app's
// About tab. Combines cargo-about (every Rust crate that ships in the binary)
// with the vendored C libraries under restorekit-sys (which aren't Cargo
// crates, so cargo-about can't see them). Run with `pnpm gen:licenses`.
import { execSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url)); // apps/desktop/scripts
const desktop = dirname(here); // apps/desktop
const srcTauri = join(desktop, "src-tauri");
const vendor = join(desktop, "../../crates/restorekit-sys/vendor");
const out = join(desktop, "src/lib/licenses.html");

const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

console.error("running cargo-about…");
const crates = execSync("cargo about generate about.hbs", {
  cwd: srcTauri,
  maxBuffer: 128 * 1024 * 1024,
}).toString();

// The C libraries statically compiled into the binary by restorekit-sys.
const cLibs = [
  ["idevicerestore", "idevicerestore", ["COPYING"], "LGPL-2.1"],
  ["libimobiledevice", "libimobiledevice", ["COPYING.LESSER"], "LGPL-2.1"],
  ["libimobiledevice-glue", "libimobiledevice-glue", ["COPYING"], "LGPL-2.1"],
  ["libirecovery", "libirecovery", ["COPYING"], "LGPL-2.1"],
  ["libplist", "libplist", ["COPYING.LESSER"], "LGPL-2.1"],
  ["libtatsu", "libtatsu", ["COPYING"], "LGPL-2.1"],
  ["libusbmuxd", "libusbmuxd", ["COPYING"], "LGPL-2.1"],
  ["libzip", "libzip", ["LICENSE"], "BSD-3-Clause"],
  ["usbmuxd (Linux and Windows builds only)", "usbmuxd", ["COPYING.GPLv2"], "GPL-2.0"],
];

let cSections = "";
for (const [label, dir, files, spdx] of cLibs) {
  const path = files.map((f) => join(vendor, dir, f)).find(existsSync);
  if (!path) {
    console.error(`warning: no license file for ${dir}`);
    continue;
  }
  cSections += `<section class="lic">
<h3 class="lic-name">${esc(spdx)}</h3>
<div class="lic-used">${esc(label)} (bundled C library)</div>
<pre class="lic-text">${esc(readFileSync(path, "utf8").trimEnd())}</pre>
</section>
`;
}

writeFileSync(out, crates + cSections);
console.error(`wrote ${out} (${cLibs.length} C libraries appended)`);
