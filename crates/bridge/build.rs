fn main() {
    println!("cargo:rerun-if-changed=src/arma_exports_x86.c");
    let architecture = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let environment = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if architecture == "x86" && environment == "msvc" {
        cc::Build::new()
            .file("src/arma_exports_x86.c")
            .warnings_into_errors(true)
            .compile("ctab_web_arma_exports_x86");
        println!("cargo:rustc-cdylib-link-arg=/WHOLEARCHIVE:ctab_web_arma_exports_x86.lib");
    }
}
