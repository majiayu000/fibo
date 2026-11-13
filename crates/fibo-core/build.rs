//! Lists the English tables under `locales/en/` for `src/i18n.rs` to compile in, so a new
//! table is a new file and nothing else.
use std::{env, fs, path::PathBuf};
fn main() {
    let dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir")).join("locales/en");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().is_some_and(|e| e == "toml"));
    files.sort();
    let mut out = String::from("&[\n");
    for p in &files {
        println!("cargo:rerun-if-changed={}", p.display());
        let name = p.file_name().unwrap().to_string_lossy();
        out.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            p.display().to_string()
        ));
    }
    out.push(']');
    let target = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("locales.rs");
    fs::write(target, out).expect("write locales.rs");
}
