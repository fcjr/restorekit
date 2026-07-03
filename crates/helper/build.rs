fn main() {
    // The XPC listener + peer-verification shim is macOS-only.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=csrc/xpc_daemon.m");
        cc::Build::new()
            .file("csrc/xpc_daemon.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("rk_xpc_daemon");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=dylib=bsm");
    }
}
