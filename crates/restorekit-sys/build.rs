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
    let zlib_include = env::var("DEP_Z_INCLUDE").ok();
    let curl_include = env::var("DEP_CURL_INCLUDE").ok();

    let deps = Deps {
        prefix: prefix.clone(),
        openssl_include,
        zlib_include,
        curl_include,
        windows: env::var_os("CARGO_CFG_WINDOWS").is_some(),
    };

    // libzip (needs zlib) — from source via CMake.
    build_libzip(&vendor.join("libzip"), &deps);

    // The libimobiledevice family, in dependency order.
    for lib in [
        "libplist",
        "libimobiledevice-glue",
        "libusbmuxd",
        "libirecovery",
        "libtatsu",
        "libimobiledevice",
    ] {
        build_autotools(&vendor.join(lib), lib, &deps);
    }

    // idevicerestore: compile its sources directly (no library upstream).
    compile_idevicerestore(&vendor.join("idevicerestore"), &deps);

    // usbmuxd server (Linux only): embed the daemon event loop so the binary
    // is self-contained — no external usbmuxd process needed.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        compile_usbmuxd(&vendor.join("usbmuxd"), &deps);
    }

    emit_link_directives(&prefix);
}

struct Deps {
    prefix: PathBuf,
    openssl_include: Option<String>,
    zlib_include: Option<String>,
    curl_include: Option<String>,
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
fn build_autotools(src: &Path, name: &str, deps: &Deps) {
    let marker = deps.prefix.join(format!(".built-{name}"));
    if marker.exists() {
        return;
    }
    println!("cargo:warning=building {name} from source");

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

    let mut configure = shell(deps.windows, "./configure");
    configure
        .current_dir(src)
        .arg(format!("--prefix={}", deps.path(&deps.prefix)))
        .arg("--enable-static")
        .arg("--disable-shared")
        .arg("--without-cython")
        .env("PKG_CONFIG_PATH", deps.pkg_config_path())
        .env("CFLAGS", deps.cflags())
        .env("CPPFLAGS", deps.cflags())
        .env("LDFLAGS", deps.ldflags());
    // A stale-timestamp maintainer-mode rebuild can re-invoke aclocal from
    // configure/make, so keep the sanitized aclocal path in scope on Windows.
    if deps.windows {
        configure.env("ACLOCAL_PATH", MSYS_ACLOCAL_PATH);
    }
    run(&mut configure, &format!("{name} configure"));

    let jobs = env::var("NUM_JOBS").unwrap_or_else(|_| "4".into());
    let mut make = Command::new("make");
    make.current_dir(src).arg(format!("-j{jobs}"));
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
        .current_dir(src)
        .arg("install")
        .arg(format!("udevrulesdir={}", udevdir.display()));
    if deps.windows {
        make_install.env("ACLOCAL_PATH", MSYS_ACLOCAL_PATH);
    }
    run(&mut make_install, &format!("{name} make install"));
    std::fs::write(&marker, "").unwrap();
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
    if let Some(inc) = &deps.zlib_include {
        cfg.define("ZLIB_INCLUDE_DIR", inc);
    }
    let dst = cfg.build();
    // cmake crate installs into <dst>; mirror into our prefix if separate.
    let _ = dst;
    std::fs::write(&marker, "").unwrap();
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

    let mut count = 0;
    for entry in std::fs::read_dir(&src_dir).unwrap() {
        let path = entry.unwrap().path();
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
        .define("HAVE_PPOLL", None)
        .define("HAVE_CLOCK_GETTIME", None)
        .define("HAVE_LOCALTIME_R", None)
        .define("PACKAGE_NAME", "\"usbmuxd\"")
        .define("PACKAGE_VERSION", "\"restorekit-embedded\"")
        .define("PACKAGE_URL", "\"https://libimobiledevice.org\"")
        .define(
            "PACKAGE_BUGREPORT",
            "\"https://github.com/libimobiledevice/usbmuxd/issues\"",
        );
    // Do NOT define HAVE_LIBIMOBILEDEVICE — avoids pulling in preflight/lockdown.

    // libusb-1.0 headers (usbmuxd includes <libusb.h> directly).
    if let Ok(output) = std::process::Command::new("pkg-config")
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
        // libusb (static, from MSYS2) plus the Win32 libraries it, libcurl, and
        // OpenSSL pull in. libusb-1.0.a lives in the MinGW prefix, not our
        // staging dir, so point rustc's linker at that libdir (via pkg-config).
        if let Some(libdir) = mingw_libusb_libdir() {
            println!("cargo:rustc-link-search=native={libdir}");
        }
        println!("cargo:rustc-link-lib=static=usb-1.0");
        for lib in [
            "setupapi", "ole32", "ws2_32", "crypt32", "secur32", "bcrypt", "iphlpapi", "userenv",
            "advapi32",
        ] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    } else {
        println!("cargo:rustc-link-lib=dylib=usb-1.0");
        println!("cargo:rustc-link-lib=dylib=pthread");
    }
}

/// Ask pkg-config where MSYS2's static libusb (`libusb-1.0.a`) lives, so the
/// final rustc link can find it. Returns a native path rustc's linker accepts.
fn mingw_libusb_libdir() -> Option<String> {
    let out = Command::new("pkg-config")
        .args(["--variable=libdir", "libusb-1.0"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!dir.is_empty()).then_some(dir)
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
