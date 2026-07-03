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

    emit_link_directives(&prefix);
}

struct Deps {
    prefix: PathBuf,
    openssl_include: Option<String>,
    zlib_include: Option<String>,
    curl_include: Option<String>,
}

impl Deps {
    /// pkg-config search path pointing at our staging prefix.
    fn pkg_config_path(&self) -> String {
        self.prefix.join("lib/pkgconfig").display().to_string()
    }

    /// Extra include dirs (staging + vendored -sys crates) as `-I` flags.
    fn cflags(&self) -> String {
        let mut flags = format!("-I{}", self.prefix.join("include").display());
        for inc in [
            &self.openssl_include,
            &self.zlib_include,
            &self.curl_include,
        ]
        .into_iter()
        .flatten()
        {
            flags.push_str(&format!(" -I{inc}"));
        }
        flags
    }

    fn ldflags(&self) -> String {
        format!("-L{}", self.prefix.join("lib").display())
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

/// Run autogen + configure + make + make install into the staging prefix.
fn build_autotools(src: &Path, name: &str, deps: &Deps) {
    let marker = deps.prefix.join(format!(".built-{name}"));
    if marker.exists() {
        return;
    }
    println!("cargo:warning=building {name} from source");

    // autogen.sh regenerates configure from a git checkout.
    if src.join("autogen.sh").exists() && !src.join("configure").exists() {
        run(
            Command::new("./autogen.sh")
                .current_dir(src)
                .env("NOCONFIGURE", "1"),
            &format!("{name} autogen"),
        );
    }

    let mut configure = Command::new("./configure");
    configure
        .current_dir(src)
        .arg(format!("--prefix={}", deps.prefix.display()))
        .arg("--enable-static")
        .arg("--disable-shared")
        .arg("--without-cython")
        .env("PKG_CONFIG_PATH", deps.pkg_config_path())
        .env("CFLAGS", deps.cflags())
        .env("CPPFLAGS", deps.cflags())
        .env("LDFLAGS", deps.ldflags());
    run(&mut configure, &format!("{name} configure"));

    let jobs = env::var("NUM_JOBS").unwrap_or_else(|_| "4".into());
    run(
        Command::new("make")
            .current_dir(src)
            .arg(format!("-j{jobs}")),
        &format!("{name} make"),
    );
    run(
        Command::new("make").current_dir(src).arg("install"),
        &format!("{name} make install"),
    );
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
        .define("CMAKE_INSTALL_PREFIX", &deps.prefix);
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
        .define("HAVE_REALPATH", None)
        .define("HAVE_STRSEP", None)
        .define("HAVE_STRCSPN", None)
        .define("HAVE_MKSTEMP", None)
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
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        for fw in ["CoreFoundation", "IOKit", "Security"] {
            println!("cargo:rustc-link-lib=framework={fw}");
        }
    } else {
        println!("cargo:rustc-link-lib=dylib=usb-1.0");
        println!("cargo:rustc-link-lib=dylib=pthread");
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
