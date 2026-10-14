use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    tauri_build::build();

    // Copy aria2c.exe to target output directories for cargo run / distribution
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src_bin = manifest_dir.join("bin").join("aria2c.exe");

    if src_bin.exists() {
        if let Ok(out_dir) = env::var("OUT_DIR") {
            let out_path = PathBuf::from(out_dir);
            let mut current = out_path.as_path();
            while let Some(parent) = current.parent() {
                if parent.file_name().map_or(false, |n| n == "debug" || n == "release") {
                    let dest = parent.join("aria2c.exe");
                    let _ = fs::copy(&src_bin, &dest);
                    break;
                }
                current = parent;
            }
        }
    }
}
