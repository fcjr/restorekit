//! Builds the idevicerestore C stack from vendored, pinned sources into a
//! staging prefix and links it statically, so the final binary is
//! self-contained (no Homebrew/apt runtime dependencies).
//!
//! External heavy deps (OpenSSL, zlib, libcurl) come from vendored Rust `-sys`
//! crates; libzip and the seven libimobiledevice-family libraries are built
//! from source here. idevicerestore itself ships no library, so its `.c`
//! sources are compiled directly (with `main` renamed out of the way).

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // docs.rs builds in a sandbox without the C toolchain, system libraries, or
    // network. `cargo doc` compiles the rlib but never links a final binary, so
    // the static C stack isn't needed there — skip building it entirely.
    if env::var_os("DOCS_RS").is_some() {
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor = manifest.join("vendor");
    ensure_submodules(&manifest, &vendor);

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let prefix = out.join("staging");
    std::fs::create_dir_all(prefix.join("lib/pkgconfig")).unwrap();
    std::fs::create_dir_all(prefix.join("include")).unwrap();

    // Includes/libs exposed by the vendored -sys build dependencies.
    let openssl_include = env::var("DEP_OPENSSL_INCLUDE").ok();
    let openssl_root = env::var("DEP_OPENSSL_ROOT").ok();
    let zlib_include = env::var("DEP_Z_INCLUDE").ok();
    let zlib_root = env::var("DEP_Z_ROOT").ok();
    let curl_include = env::var("DEP_CURL_INCLUDE").ok();
    let curl_root = env::var("DEP_CURL_ROOT").ok();

    let deps = Deps {
        prefix: prefix.clone(),
        openssl_include,
        openssl_root,
        zlib_include,
        zlib_root,
        curl_include,
        curl_root,
        windows: env::var_os("CARGO_CFG_WINDOWS").is_some(),
    };

    write_openssl_pc(&deps);

    // libzip (needs zlib) — from source via CMake.
    build_libzip(&vendor.join("libzip"), &deps);

    // The libimobiledevice family, in dependency order. The version is the
    // release tag each submodule is pinned at (see `git submodule status`); it
    // seeds `.tarball-version` when building outside a git checkout and must
    // satisfy the projects' inter-package `PKG_CHECK_MODULES` minimums.
    for (lib, version) in [
        ("libplist", "2.7.0"),
        ("libimobiledevice-glue", "1.3.2"),
        ("libusbmuxd", "2.1.1"),
        ("libirecovery", "1.4.0"),
        ("libtatsu", "1.0.5"),
        ("libimobiledevice", "1.4.0"),
    ] {
        let src = if manifest.join("patches").join(lib).is_dir() {
            patch_autotools_sources(&manifest, lib, &vendor.join(lib), &out, &deps)
        } else {
            vendor.join(lib)
        };
        build_autotools(&src, lib, version, &deps);
    }

    // idevicerestore: compile its sources directly (no library upstream),
    // after copying them aside and applying our patches (see patches/).
    let idevicerestore_src =
        patch_idevicerestore_sources(&manifest, &vendor.join("idevicerestore"), &out);
    compile_idevicerestore(&idevicerestore_src, &deps);

    // usbmuxd server (Linux + Windows): embed the daemon event loop so the
    // binary is self-contained — no external usbmuxd process needed. It's what
    // idevicerestore talks to for the restore-mode phase.
    let target_os = env::var("CARGO_CFG_TARGET_OS");
    if matches!(target_os.as_deref(), Ok("linux") | Ok("windows")) {
        // Linux links the system libusb. MSYS2 no longer packages libusb for
        // MINGW64, so on Windows build it from the vendored source; only the
        // embedded usbmuxd uses it there (libirecovery talks to Windows
        // directly), so it goes after the libimobiledevice family.
        if deps.windows {
            build_autotools(&vendor.join("libusb"), "libusb", "1.0.30", &deps);
        }
        let usbmuxd_src =
            patch_autotools_sources(&manifest, "usbmuxd", &vendor.join("usbmuxd"), &out, &deps);
        compile_usbmuxd(&usbmuxd_src, &deps);
        // The marker patch_autotools_sources checks before reusing the copy;
        // build_autotools writes it for the other libraries.
        std::fs::write(prefix.join(".built-usbmuxd"), "").unwrap();
    }

    emit_link_directives(&prefix);
}

struct Deps {
    prefix: PathBuf,
    openssl_include: Option<String>,
    /// openssl-src's install, with the static libssl/libcrypto in `lib/`.
    openssl_root: Option<String>,
    zlib_include: Option<String>,
    /// libz-sys's build, with the static zlib in `lib/`.
    zlib_root: Option<String>,
    curl_include: Option<String>,
    /// curl-sys's build, with the static libcurl in `build/`.
    curl_root: Option<String>,
    /// Building for a Windows target (autotools run under MSYS2/MinGW, which
    /// wants POSIX-style paths).
    windows: bool,
}

impl Deps {
    /// A path in the form the C toolchain expects: MSYS2 POSIX on Windows,
    /// native otherwise.
    fn path(&self, p: &Path) -> String {
        if self.windows {
            to_msys(p)
        } else {
            p.display().to_string()
        }
    }

    /// pkg-config search path pointing at our staging prefix.
    fn pkg_config_path(&self) -> String {
        self.path(&self.prefix.join("lib/pkgconfig"))
    }

    /// Extra include dirs (staging + vendored -sys crates) as `-I` flags.
    fn cflags(&self) -> String {
        let mut flags = format!("-I{}", self.path(&self.prefix.join("include")));
        for inc in [
            &self.openssl_include,
            &self.zlib_include,
            &self.curl_include,
        ]
        .into_iter()
        .flatten()
        {
            flags.push_str(&format!(" -I{}", self.path(Path::new(inc))));
        }
        // Force the whole family's headers into static-linkage mode (no
        // `__declspec(dllimport)`) for every library we configure. CURL_STATICLIB
        // does the same for <curl/curl.h> (libtatsu links our static libcurl).
        if self.windows {
            for def in WINDOWS_STATIC_DEFINES {
                flags.push_str(&format!(" -D{def}"));
            }
            flags.push_str(" -DCURL_STATICLIB");
            // 64-bit file offsets: mingw's default `stat`/`off_t` are 32-bit, so
            // anything touching a >2 GB file (e.g. a ~20 GB IPSW) fails with
            // EOVERFLOW without this.
            flags.push_str(" -D_FILE_OFFSET_BITS=64");
        }
        // When the final artifact is a shared library (e.g. Tauri on Linux),
        // all static C code must be position-independent.
        if !self.windows {
            flags.push_str(" -fPIC");
        }
        flags
    }

    fn ldflags(&self) -> String {
        format!("-L{}", self.path(&self.prefix.join("lib")))
    }

    fn openssl_lib_dir(&self) -> Option<PathBuf> {
        self.openssl_root.as_ref().map(|r| Path::new(r).join("lib"))
    }

    /// curl-sys compiles libcurl with `cc` into `build/`, not `lib/`.
    fn curl_lib_dir(&self) -> Option<PathBuf> {
        self.curl_root.as_ref().map(|r| Path::new(r).join("build"))
    }
}

/// Stage an `openssl.pc` for the bundled OpenSSL.
///
/// configure is handed OpenSSL's flags directly, but libimobiledevice still
/// writes `openssl >= 0.9.8` into its installed .pc's `Requires.private`, so
/// without this a pkg-config query against the staging prefix fails on a host
/// with no system OpenSSL.
fn write_openssl_pc(deps: &Deps) {
    let (Some(include), Some(lib_dir)) = (&deps.openssl_include, deps.openssl_lib_dir()) else {
        return;
    };
    let header = std::fs::read_to_string(Path::new(include).join("openssl/opensslv.h"))
        .expect("read bundled opensslv.h");
    let version = header
        .lines()
        .find_map(|l| {
            l.trim_start_matches(['#', ' '])
                .strip_prefix("define OPENSSL_VERSION_STR")
                .map(|v| v.trim().trim_matches('"').to_string())
        })
        .expect("OPENSSL_VERSION_STR in bundled opensslv.h");
    let pc = format!(
        "Name: OpenSSL\n\
         Description: Bundled OpenSSL (openssl-src)\n\
         Version: {version}\n\
         Libs: -L{lib} -lssl -lcrypto\n\
         Cflags: -I{inc}\n",
        lib = deps.path(&lib_dir),
        inc = deps.path(Path::new(include)),
    );
    std::fs::write(deps.prefix.join("lib/pkgconfig/openssl.pc"), pc).unwrap();
}

/// The libimobiledevice family decorates its public API with
/// `__declspec(dllimport)` on Windows unless a per-library `*_STATIC` macro is
/// defined. We link the whole stack statically, so every consumer (each library
/// that includes an upstream header, plus idevicerestore) must define *all* of
/// these — otherwise the compiler emits `__imp_`-prefixed references that don't
/// exist in the static archives and the link fails. Each library's own
/// configure only sets its own macro, so we supply the full set globally.
const WINDOWS_STATIC_DEFINES: &[&str] = &[
    "LIBPLIST_STATIC",
    "LIMD_GLUE_STATIC",
    "LIBUSBMUXD_STATIC",
    "IRECV_STATIC",
    "LIBTATSU_STATIC",
    "LIBIMOBILEDEVICE_STATIC",
];

/// Translate `C:\a\b` → `/c/a/b` for MSYS2 tools.
fn to_msys(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let b = s.as_bytes();
    if b.len() >= 2 && b[1] == b':' {
        format!("/{}{}", (b[0] as char).to_ascii_lowercase(), &s[2..])
    } else {
        s
    }
}

fn ensure_submodules(manifest: &Path, vendor: &Path) {
    // If the pinned sources are missing (fresh clone without --recurse), fetch them.
    if vendor.join("idevicerestore/src/idevicerestore.c").exists() {
        return;
    }
    // Walk up to the repo root to run submodule update.
    let repo_root = manifest
        .ancestors()
        .find(|p| p.join(".git").exists())
        .unwrap_or(manifest);
    let status = Command::new("git")
        .args(["submodule", "update", "--init", "--recursive"])
        .current_dir(repo_root)
        .status();
    if !matches!(status, Ok(s) if s.success()) {
        panic!(
            "vendored C sources missing and `git submodule update --init` failed; \
             run it manually in {}",
            repo_root.display()
        );
    }
}

/// Clean POSIX aclocal search path for the MSYS2 autotools.
///
/// Native cargo inherits `ACLOCAL_PATH` from the launching MSYS2 shell already
/// translated to Windows form (`C:\msys64\mingw64\share\aclocal;...`). When
/// build.rs re-enters MSYS2 via `sh`, aclocal reads that value back as a bogus
/// `/msys64/usr/share/aclocal` prefix and dies looking for its own macros
/// (e.g. `progtest.m4`), which aborts autoreconf. Forcing the correct POSIX
/// dirs sidesteps the Win32↔POSIX round-trip. `/usr/share/aclocal` holds the
/// automake/gettext macros; `/mingw64/share/aclocal` holds `pkg.m4` et al.
const MSYS_ACLOCAL_PATH: &str = "/usr/share/aclocal:/mingw64/share/aclocal";

/// Run autogen + configure + make + make install into the staging prefix.
fn build_autotools(src: &Path, name: &str, version: &str, deps: &Deps) {
    let marker = deps.prefix.join(format!(".built-{name}"));
    if marker.exists() {
        return;
    }
    println!("cargo:warning=building {name} from source");

    // Build out-of-tree in a per-OUT_DIR directory rather than in the vendored
    // source. libtool bakes the absolute install `libdir` into each `.la` file
    // at link time; an in-place build in the shared vendor tree lets one build
    // unit's OUT_DIR leak into another's `.la`, and since `cargo clean` only
    // wipes `target/`, stale paths survive there. A private build dir keeps
    // every unit (profile, workspace) self-contained and reproducible.
    let build_dir = deps.prefix.parent().unwrap().join("build").join(name);
    std::fs::create_dir_all(&build_dir).unwrap();

    // These projects derive PACKAGE_VERSION from `git describe`, falling back
    // to a `.tarball-version` file. The published crate ships the vendored
    // sources without their submodule `.git` links, so configure would abort
    // with "PACKAGE_VERSION is not defined" — supply the fallback.
    if !src.join(".git").exists() && !src.join(".tarball-version").exists() {
        std::fs::write(src.join(".tarball-version"), version).unwrap();
    }

    // Regenerate configure from the git checkout. On Windows we drive
    // `autoreconf` directly — the projects' hand-rolled autogen.sh runs a bare
    // `aclocal -I m4` that trips over MSYS2's gettext macro layout; autoreconf
    // discovers and orders the macros robustly. Elsewhere autogen.sh is fine.
    if !src.join("configure").exists() {
        if deps.windows {
            run(
                Command::new("sh")
                    .arg("-c")
                    .arg("autoreconf --install --force")
                    .current_dir(src)
                    .env("ACLOCAL_PATH", MSYS_ACLOCAL_PATH),
                &format!("{name} autoreconf"),
            );
        } else if src.join("autogen.sh").exists() {
            run(
                Command::new("./autogen.sh")
                    .current_dir(src)
                    .env("NOCONFIGURE", "1"),
                &format!("{name} autogen"),
            );
        }
    }

    let mut configure = shell(deps.windows, &deps.path(&src.join("configure")));
    configure
        .current_dir(&build_dir)
        .arg(format!("--prefix={}", deps.path(&deps.prefix)))
        .arg("--enable-static")
        .arg("--disable-shared")
        .arg("--without-cython")
        .arg("--without-tools")
        .env("PKG_CONFIG_PATH", deps.pkg_config_path())
        .env("CFLAGS", deps.cflags())
        .env("CPPFLAGS", deps.cflags())
        .env("LDFLAGS", deps.ldflags());
    // configure takes the bundled OpenSSL and curl as given rather than
    // looking for a system copy through pkg-config. curl-sys's own libcurl.pc
    // can't stand in: it leaves curl 8's `@LIBCURL_PC_*@` placeholders
    // unfilled. The `-L` keeps a link through these flags (they end up in the
    // libtool dependency libs) from finding a system libssl or libcurl.
    for (module, include, lib_dir, libs) in [
        (
            "openssl",
            &deps.openssl_include,
            deps.openssl_lib_dir(),
            "-lssl -lcrypto",
        ),
        ("libcurl", &deps.curl_include, deps.curl_lib_dir(), "-lcurl"),
    ] {
        if let (Some(include), Some(lib_dir)) = (include, lib_dir) {
            configure
                .env(
                    format!("{module}_CFLAGS"),
                    format!("-I{}", deps.path(Path::new(include))),
                )
                .env(
                    format!("{module}_LIBS"),
                    format!("-L{} {libs}", deps.path(&lib_dir)),
                );
        }
    }
    // A stale-timestamp maintainer-mode rebuild can re-invoke aclocal from
    // configure/make, so keep the sanitized aclocal path in scope on Windows.
    if deps.windows {
        // Pin the host type. config.guess reports x86_64-w64-mingw32 inside the
        // MSYS2 MINGW64 shell but x86_64-pc-mingw64 from a plain Windows shell
        // (the desktop release builds from PowerShell), and libirecovery only
        // picks its native Windows backend for `mingw32*`; otherwise it wants
        // libusb, which MSYS2 no longer packages for MINGW64.
        configure
            .arg("--build=x86_64-w64-mingw32")
            .arg("--host=x86_64-w64-mingw32");
        configure.env("ACLOCAL_PATH", MSYS_ACLOCAL_PATH);
    }
    run(&mut configure, &format!("{name} configure"));
    if deps.windows {
        drop_libtool_globs(&build_dir.join("libtool"));
    }

    // The libraries only: nothing here runs the projects' programs or tests.
    // libplist and libirecovery leave theirs out with `--without-tools`;
    // libusbmuxd and libimobiledevice have no such switch, so make gets an
    // empty list.
    let libraries_only = ["bin_PROGRAMS="];
    let jobs = env::var("NUM_JOBS").unwrap_or_else(|_| "4".into());
    let mut make = Command::new("make");
    make.current_dir(&build_dir)
        .arg(format!("-j{jobs}"))
        .args(libraries_only);
    if deps.windows {
        make.env("ACLOCAL_PATH", MSYS_ACLOCAL_PATH);
    }
    run(&mut make, &format!("{name} make"));
    // libirecovery installs a udev rule to a system dir by default, which fails
    // for a non-root user (e.g. CI). Redirect it into our staging prefix.
    let udevdir = deps.prefix.join("udev");
    std::fs::create_dir_all(&udevdir).ok();
    let mut make_install = Command::new("make");
    make_install
        .current_dir(&build_dir)
        .arg("install")
        .arg(format!("udevrulesdir={}", udevdir.display()))
        .args(libraries_only);
    if deps.windows {
        make_install.env("ACLOCAL_PATH", MSYS_ACLOCAL_PATH);
    }
    run(&mut make_install, &format!("{name} make install"));
    std::fs::write(&marker, "").unwrap();
}

/// Take the file globs out of the generated `libtool` script's
/// convenience-library extraction (Windows only).
///
/// To fold a convenience library into a static one, libtool unpacks its members
/// and collects them back with a backslash-escaped pattern:
///
/// ```sh
/// my_oldobjs="$my_oldobjs "`find $my_xdir -name \*.$objext -print -o -name \*.lo -print | ...`
/// ```
///
/// When cargo (a native Windows process) drives the build rather than the
/// MINGW64 login shell, `*.o` reaches find expanded against the build directory
/// anyway. Quoting the pattern in the script didn't help (v0.5.11 and v0.5.12
/// failed the same way), so the expansion happens on find's side, past any
/// shell quoting. find then bails with "paths must precede expression", and
/// because libtool never checks, the member list comes back empty: the archive
/// is created without them and the build limps on. libplist ships libcnary that
/// way, so `libplist-2.0.a` loses node.o, and the first thing to link plist_*
/// dies on `undefined reference to 'node_first_child'`. With the projects'
/// own tools no longer built, that's the final link of the Rust binary.
///
/// The extraction directory only ever holds the members `ar x` just unpacked,
/// so list every file instead and leave find nothing to glob. Not done
/// elsewhere: BSD ar on macOS also extracts its `__.SYMDEF` symbol table.
fn drop_libtool_globs(libtool: &Path) {
    let Ok(script) = std::fs::read_to_string(libtool) else {
        return;
    };
    let patched = script.replace(
        r"-name \*.$objext -print -o -name \*.lo -print",
        "-type f -print",
    );
    if patched != script {
        std::fs::write(libtool, patched).unwrap();
    }
}

fn build_libzip(src: &Path, deps: &Deps) {
    let marker = deps.prefix.join(".built-libzip");
    if marker.exists() {
        return;
    }
    println!("cargo:warning=building libzip from source");
    let mut cfg = cmake::Config::new(src);
    cfg.define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_TOOLS", "OFF")
        .define("BUILD_EXAMPLES", "OFF")
        .define("BUILD_DOC", "OFF")
        .define("BUILD_REGRESS", "OFF")
        .define("ENABLE_COMMONCRYPTO", "OFF")
        .define("ENABLE_GNUTLS", "OFF")
        .define("ENABLE_MBEDTLS", "OFF")
        .define("ENABLE_OPENSSL", "OFF")
        .define("ENABLE_BZIP2", "OFF")
        .define("ENABLE_LZMA", "OFF")
        .define("ENABLE_ZSTD", "OFF")
        .define("CMAKE_INSTALL_PREFIX", &deps.prefix)
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON");
    if let Some(root) = &deps.zlib_root {
        cfg.define("ZLIB_ROOT", root);
    }
    let dst = cfg.build();
    // cmake crate installs into <dst>; mirror into our prefix if separate.
    let _ = dst;
    std::fs::write(&marker, "").unwrap();
}

/// Copy idevicerestore's `src/` into OUT_DIR and apply the `.patch` files from
/// `patches/idevicerestore/` in filename order. Patching a throwaway copy
/// keeps the vendored submodule pristine (clean `git status`, clean submodule
/// pins) while the patches live in this repo, reviewed and versioned. Returns
/// the patched project root (containing `src/`).
fn patch_idevicerestore_sources(manifest: &Path, vendor_src: &Path, out: &Path) -> PathBuf {
    let patch_dir = manifest.join("patches/idevicerestore");
    println!("cargo:rerun-if-changed={}", patch_dir.display());

    let root = out.join("idevicerestore-patched");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let dst = root.join("src");
    std::fs::create_dir_all(&dst).unwrap();
    for entry in std::fs::read_dir(vendor_src.join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            std::fs::copy(&path, dst.join(path.file_name().unwrap())).unwrap();
        }
    }

    let mut patches: Vec<PathBuf> = std::fs::read_dir(&patch_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("patch"))
        .collect();
    patches.sort();
    for patch in patches {
        println!("cargo:rerun-if-changed={}", patch.display());
        // Apply with POSIX `patch`, not `git apply`. OUT_DIR lives under this
        // repo's `target/`, which is git-ignored, and `git apply` run from
        // inside a work tree silently *skips* patches whose target paths are
        // ignored (it prints "Skipped patch" but still exits 0), leaving the
        // sources unpatched. `patch` has no repo awareness, so it applies them
        // regardless of the surrounding worktree.
        run(
            Command::new("patch")
                .args(["-p1", "--no-backup-if-mismatch", "-i"])
                .arg(&patch)
                .current_dir(&root),
            &format!("apply {}", patch.file_name().unwrap().to_string_lossy()),
        );
    }
    root
}

/// Copy an autotools library into OUT_DIR and apply `patches/<name>/` to it,
/// for the same reason as [`patch_idevicerestore_sources`]. It's only re-copied
/// and rebuilt when the patches change.
/// The stamp lives in the staging prefix next to build_autotools' marker, so a
/// CI cache that restores only the prefix still skips the rebuild.
fn patch_autotools_sources(
    manifest: &Path,
    name: &str,
    vendor_src: &Path,
    out: &Path,
    deps: &Deps,
) -> PathBuf {
    use std::hash::{Hash, Hasher};

    let patch_dir = manifest.join("patches").join(name);
    println!("cargo:rerun-if-changed={}", patch_dir.display());
    let mut patches: Vec<PathBuf> = std::fs::read_dir(&patch_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("patch"))
        .collect();
    patches.sort();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for patch in &patches {
        println!("cargo:rerun-if-changed={}", patch.display());
        std::fs::read(patch).unwrap().hash(&mut hasher);
    }
    let stamp = format!("{:016x}", hasher.finish());

    let root = out.join(format!("{name}-patched"));
    let stamp_file = deps.prefix.join(format!(".{name}-patches"));
    let marker = deps.prefix.join(format!(".built-{name}"));
    let built = marker.exists() && std::fs::read_to_string(&stamp_file).is_ok_and(|s| s == stamp);
    if built && root.exists() {
        return root;
    }

    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    copy_tree(vendor_src, &root);
    for patch in &patches {
        // POSIX `patch`, not `git apply`; see patch_idevicerestore_sources.
        run(
            Command::new("patch")
                .args(["-p1", "--no-backup-if-mismatch", "-i"])
                .arg(patch)
                .current_dir(&root),
            &format!("apply {}", patch.file_name().unwrap().to_string_lossy()),
        );
    }
    // New patches force a fresh configure + build. A prefix cached without
    // the copy (CI's C-stack cache) only needed the copy back.
    if !built {
        let _ = std::fs::remove_file(&marker);
        let _ = std::fs::remove_dir_all(deps.prefix.parent().unwrap().join("build").join(name));
        std::fs::write(&stamp_file, stamp).unwrap();
    }
    root
}

/// Recursively copy `src` to `dst`, skipping `.git` (a submodule's `.git` is a
/// file pointing back into the superproject, which a copy can't follow) and
/// autotools output left by an earlier in-tree autogen. A copy gets fresh
/// mtimes, so stale generated files could look out of date and make `make`
/// re-run automake; leaving them out makes build_autotools regenerate them.
/// Symlinks are skipped too: the upstream trees track none, but autogen on
/// macOS links its aux files (`ltmain.sh`, `config.guess`, …) into Homebrew.
fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if matches!(
            name.to_str(),
            Some(
                ".git"
                    | "autom4te.cache"
                    | "configure"
                    | "aclocal.m4"
                    | "Makefile.in"
                    | "config.h.in"
            )
        ) {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type().unwrap();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            copy_tree(&path, &dst.join(&name));
        } else {
            std::fs::copy(&path, dst.join(&name)).unwrap();
        }
    }
}

/// Compile idevicerestore's own C sources into a static archive and link it.
fn compile_idevicerestore(src: &Path, deps: &Deps) {
    let src_dir = src.join("src");
    let mut build = cc::Build::new();
    build
        .include(&src_dir)
        .include(deps.prefix.join("include"))
        // idevicerestore uses -Wno-multichar for its FourCC constants.
        .flag_if_supported("-Wno-multichar")
        .flag_if_supported("-Wno-deprecated-declarations")
        // Rename idevicerestore.c's main() so it doesn't clash with Rust's.
        .define("main", "idevicerestore_cli_main_unused")
        .define("HAVE_OPENSSL", None)
        // Function-detection defines normally emitted into autotools' config.h.
        // strcspn is standard C and present everywhere (incl. mingw).
        .define("HAVE_STRCSPN", None)
        // Feature checks configure.ac runs against libimobiledevice; our
        // vendored copy (1.4.0) has all of them. Without HAVE_IDEVICE_E_TIMEOUT
        // a quiet FDR channel returns IDEVICE_E_TIMEOUT (-7) which fdr.c then
        // treats as fatal, killing restores mid-transfer; HAVE_REVERSE_PROXY
        // selects libimobiledevice's reverse proxy over the legacy FDR path.
        .define("HAVE_IDEVICE_E_TIMEOUT", None)
        .define("HAVE_RESTORE_E_RECEIVE_TIMEOUT", None)
        .define("HAVE_ENUM_IDEVICE_CONNECTION_TYPE", None)
        .define("HAVE_REVERSE_PROXY", None)
        .define("PACKAGE_NAME", "\"idevicerestore\"")
        .define("PACKAGE_VERSION", "\"restorekit-vendored\"")
        .define("PACKAGE_STRING", "\"idevicerestore (restorekit)\"")
        .define("PACKAGE_URL", "\"https://libimobiledevice.org\"")
        .define(
            "PACKAGE_BUGREPORT",
            "\"https://github.com/libimobiledevice/idevicerestore/issues\"",
        );
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build.define("_DARWIN_BETTER_REALPATH", None);
    }
    if deps.windows {
        // idevicerestore includes the family's headers; match the static
        // linkage the libraries were built with so it doesn't emit dllimport
        // references.
        for def in WINDOWS_STATIC_DEFINES {
            build.define(def, None);
        }
        // Link our static libcurl without dllimport stubs (see cflags()).
        build.define("CURL_STATICLIB", None);
        // 64-bit file offsets so stat()/seek on a ~20 GB IPSW don't overflow
        // mingw's 32-bit defaults (see cflags()).
        build.define("_FILE_OFFSET_BITS", "64");
        // realpath/strsep/mkstemp are absent on mingw; idevicerestore ships
        // WIN32 fallbacks guarded by `#ifndef HAVE_*`, so leave these undefined.
    } else {
        build
            .define("HAVE_REALPATH", None)
            .define("HAVE_STRSEP", None)
            .define("HAVE_MKSTEMP", None);
    }
    for inc in [
        &deps.openssl_include,
        &deps.zlib_include,
        &deps.curl_include,
    ]
    .into_iter()
    .flatten()
    {
        build.include(inc);
    }

    // deviceinterfaced.c is macOS-only (posix_spawn/launchctl); upstream's
    // Makefile.am builds it only on Darwin, where main() references it.
    let is_macos = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos");
    let mut count = 0;
    for entry in std::fs::read_dir(&src_dir).unwrap() {
        let path = entry.unwrap().path();
        if !is_macos && path.file_name().and_then(|n| n.to_str()) == Some("deviceinterfaced.c") {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("c") {
            build.file(&path);
            count += 1;
        }
    }
    assert!(
        count > 0,
        "no idevicerestore .c sources found in {src_dir:?}"
    );

    // Our log-capture shim (routes idevicerestore's logger to a Rust sink).
    let shim = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("csrc/log_capture.c");
    println!("cargo:rerun-if-changed={}", shim.display());
    build.file(&shim);

    build.compile("idevicerestore");
}

/// Compile usbmuxd's server sources (minus main.c/preflight.c) plus our shim
/// into a static archive so restorekit can run an in-process usbmuxd on Linux.
fn compile_usbmuxd(src: &Path, deps: &Deps) {
    let src_dir = src.join("src");
    let mut build = cc::Build::new();
    build
        .include(&src_dir)
        .include(deps.prefix.join("include"))
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-sign-compare")
        // usbmuxd's log.c defines `unsigned int log_level` which clashes with
        // idevicerestore's identically-named global. Rename it at the
        // preprocessor level to avoid a linker duplicate-symbol error.
        .define("log_level", "usbmuxd_log_level")
        .define("PACKAGE_NAME", "\"usbmuxd\"")
        .define("PACKAGE_VERSION", "\"restorekit-embedded\"")
        .define("PACKAGE_URL", "\"https://libimobiledevice.org\"")
        .define(
            "PACKAGE_BUGREPORT",
            "\"https://github.com/libimobiledevice/usbmuxd/issues\"",
        );
    // Do NOT define HAVE_LIBIMOBILEDEVICE — avoids pulling in preflight/lockdown.

    if deps.windows {
        // POSIX→Winsock compat shim headers first, plus the family's static
        // macros and 64-bit file offsets (see cflags()).
        build.include(PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("csrc/win_shim"));
        for def in WINDOWS_STATIC_DEFINES {
            build.define(def, None);
        }
        build
            .define("_FILE_OFFSET_BITS", "64")
            .define("WIN32_LEAN_AND_MEAN", None)
            // Our win_shim provides a localtime_r that bridges Winsock's 32-bit
            // tv_sec; take that path rather than the plain-localtime one.
            .define("HAVE_LOCALTIME_R", None)
            // mingw-w64 has clock_gettime; use it instead of utils.c's macOS
            // mach_absolute_time fallback.
            .define("HAVE_CLOCK_GETTIME", None);
    } else {
        build
            .define("HAVE_PPOLL", None)
            .define("HAVE_CLOCK_GETTIME", None)
            .define("HAVE_LOCALTIME_R", None);
    }

    // libusb-1.0 headers (usbmuxd includes <libusb.h> directly): our own build
    // on Windows, the system's elsewhere.
    if deps.windows {
        build.include(deps.prefix.join("include/libusb-1.0"));
    } else if let Ok(output) = std::process::Command::new("pkg-config")
        .args(["--cflags-only-I", "libusb-1.0"])
        .output()
    {
        let flags = String::from_utf8_lossy(&output.stdout);
        for flag in flags.split_whitespace() {
            if let Some(dir) = flag.strip_prefix("-I") {
                build.include(dir);
            }
        }
    }

    let skip = ["main.c", "preflight.c"];
    let mut count = 0;
    for entry in std::fs::read_dir(&src_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("c") {
            let name = path.file_name().unwrap().to_str().unwrap();
            if skip.contains(&name) {
                continue;
            }
            build.file(&path);
            count += 1;
        }
    }
    assert!(count > 0, "no usbmuxd .c sources found in {src_dir:?}");

    // Our shim that provides main-loop functions + preflight stubs.
    let shim = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("csrc/usbmuxd_server.c");
    println!("cargo:rerun-if-changed={}", shim.display());
    build.file(&shim);

    build.compile("usbmuxd_server");
}

fn emit_link_directives(prefix: &Path) {
    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );

    // Static libimobiledevice stack + libzip, most-dependent first.
    for lib in [
        "imobiledevice-1.0",
        "tatsu",
        "irecovery-1.0",
        "usbmuxd-2.0",
        "imobiledevice-glue-1.0",
        "plist-2.0",
        "zip",
    ] {
        println!("cargo:rustc-link-lib=static={lib}");
    }

    // openssl/zlib/curl come from the vendored -sys crates' own link directives.

    // System libraries and frameworks.
    let target_os = env::var("CARGO_CFG_TARGET_OS");
    if target_os.as_deref() == Ok("macos") {
        for fw in ["CoreFoundation", "IOKit", "Security"] {
            println!("cargo:rustc-link-lib=framework={fw}");
        }
    } else if target_os.as_deref() == Ok("windows") {
        // libusb (static, built into our staging prefix) plus the Win32
        // libraries it, libcurl, and OpenSSL pull in.
        println!("cargo:rustc-link-lib=static=usb-1.0");
        for lib in [
            "setupapi", "ole32", "ws2_32", "crypt32", "secur32", "bcrypt", "iphlpapi", "userenv",
            "advapi32", "winusb",
        ] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    } else {
        println!("cargo:rustc-link-lib=dylib=usb-1.0");
        println!("cargo:rustc-link-lib=dylib=pthread");
    }
}

/// A Command that runs `script` — directly on Unix, or via MSYS2's `sh` on
/// Windows (where Rust's `Command` can't exec a shell script itself).
fn shell(windows: bool, script: &str) -> Command {
    if windows {
        let mut c = Command::new("sh");
        c.arg(script);
        c
    } else {
        Command::new(script)
    }
}

fn run(cmd: &mut Command, what: &str) {
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {what}: {e}"));
    if !status.success() {
        panic!("{what} failed with {status}");
    }
}
