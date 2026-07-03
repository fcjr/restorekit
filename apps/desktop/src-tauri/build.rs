fn main() {
    // App-side privileged-helper glue (SMAppService + XPC client), macOS-only.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=csrc/privhelper.m");
        cc::Build::new()
            .file("csrc/privhelper.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("rk_privhelper");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=ServiceManagement");
    }

    tauri_build::build()
}
