//! Copies the built web UI (web/dist) into OUT_DIR for embedding, or writes a
//! one-page placeholder when the UI has not been built, so `cargo build` works
//! without Node. The embedded copy lives in OUT_DIR so this script never writes
//! into the source tree. `KANJO_WEB_DIST` points at another build of the UI.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const PLACEHOLDER: &str =
    "<!doctype html><html><head><meta charset=\"utf-8\"><title>kanjo</title></head><body>UI not built</body></html>\n";

fn main() -> io::Result<()> {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| io::Error::other("CARGO_MANIFEST_DIR"))?);
    let out = PathBuf::from(std::env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("OUT_DIR"))?);
    let dist = std::env::var_os("KANJO_WEB_DIST").map(PathBuf::from).unwrap_or_else(|| manifest.join("../../web/dist"));
    let target = out.join("web");
    // A path that does not exist makes cargo rerun this script on every build, so
    // while the UI is unbuilt the parent directory (web/) is watched instead.
    let watched = match dist.parent() {
        Some(parent) if !dist.exists() && parent.is_dir() && parent.parent().is_some() => parent.to_path_buf(),
        _ => dist.clone(),
    };
    println!("cargo:rerun-if-changed={}", watched.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=KANJO_WEB_DIST");
    let mut wanted = Vec::new();
    if dist.join("index.html").is_file() {
        collect(&dist, Path::new(""), &mut wanted)?;
    } else {
        println!("cargo:warning=web/dist not found; embedding a placeholder page (run `cd web && npm ci && npm run build`)");
        wanted.push((PathBuf::from("index.html"), PLACEHOLDER.as_bytes().to_vec()));
    }
    sync(&target, &wanted)?;
    println!("cargo:rustc-env=KANJO_WEB_DIR={}", target.display());
    Ok(())
}

/// Reads every file under `dir` as (relative path, bytes).
fn collect(dir: &Path, rel: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = rel.join(entry.file_name());
        if kind.is_dir() {
            collect(&entry.path(), &name, out)?;
        } else if kind.is_file() {
            out.push((name, fs::read(entry.path())?));
        }
    }
    Ok(())
}

/// Makes `target` hold exactly `wanted`, writing only files whose content
/// changed, so an unchanged UI does not make the crate recompile.
fn sync(target: &Path, wanted: &[(PathBuf, Vec<u8>)]) -> io::Result<()> {
    let mut existing = Vec::new();
    if target.is_dir() {
        collect(target, Path::new(""), &mut existing)?;
    }
    for (rel, _) in &existing {
        if !wanted.iter().any(|(w, _)| w == rel) {
            fs::remove_file(target.join(rel))?;
        }
    }
    for (rel, bytes) in wanted {
        let dest = target.join(rel);
        if fs::read(&dest).is_ok_and(|old| &old == bytes) {
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, bytes)?;
    }
    Ok(())
}
