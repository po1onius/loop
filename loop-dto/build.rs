use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let app_dir = manifest_dir
        .parent()
        .expect("loop-dto should be in workspace root")
        .join("app")
        .join("lib");

    std::fs::create_dir_all(&app_dir).expect("failed to ensure app directory exists");

    println!("cargo:rustc-env=TS_RS_EXPORT_DIR={}", app_dir.display());
    println!("cargo:rerun-if-changed=build.rs");
}
