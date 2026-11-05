fn main() {
    tauri_build::build();

    // Workaround for STATUS_ENTRYPOINT_NOT_FOUND (0xC0000139) at process start when
    // test/example binaries link tauri GUI code (TaskDialogIndirect): without the
    // Common-Controls v6 manifest the loader binds comctl32 v5 (no TaskDialogIndirect).
    // Mirrors tauri's own embed_manifest_for_tests() (tauri/build.rs), which only runs
    // inside the tauri workspace (__TAURI_WORKSPACE__), so downstream crates must do it.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        for arg in [
            "/MANIFEST:EMBED".to_string(),
            format!("/MANIFESTINPUT:{}", manifest.display()),
        ] {
            println!("cargo:rustc-link-arg-tests={arg}");
            println!("cargo:rustc-link-arg-examples={arg}");
        }
    }
}
