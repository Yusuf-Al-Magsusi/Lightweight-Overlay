//! Embeds `app.manifest` into the executable: themed controls for the settings
//! dialog, an explicit "never ask for administrator rights" execution level,
//! and per-monitor DPI awareness. No extra crate needed; the MSVC linker does it.

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=build.rs");
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|v| v == "windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|v| v == "msvc");
    if windows_msvc {
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{dir}\\app.manifest");
    }
}
