//! Locates and links the c-toxcore static library.
//!
//! Search order:
//!   1. `TOXCORE_LIB` env var (explicit path to directory containing toxcore.lib)
//!   2. `<workspace>/third_party/c-toxcore/build` (default CMake build dir)
//!
//! The c-toxcore static library (MSVC: `toxcore.lib`) references libsodium,
//! so the caller must also make the sodium lib available on the link path
//! (e.g. via `vcpkg integrate install` or the `SODIUM_LIB` env var).

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // <workspace>/crates/tox-ffi -> <workspace>
    let workspace = manifest.parent().and_then(|p| p.parent()).unwrap();

    let candidates: Vec<PathBuf> = vec![
        env::var("TOXCORE_LIB").ok().map(PathBuf::from),
        Some(workspace.join("third_party/c-toxcore/build")),
        Some(workspace.join("third_party/c-toxcore/_build")),
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut found: Option<PathBuf> = None;
    for dir in &candidates {
        let lib = dir.join("toxcore.lib");
        let a = dir.join("libtoxcore.a");
        if lib.exists() || a.exists() {
            found = Some(dir.clone());
            break;
        }
    }

    match found {
        Some(dir) => {
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=static=toxcore");
            // Windows system deps required by c-toxcore.
            for lib in ["ws2_32", "iphlpapi", "advapi32", "user32", "shell32", "bcrypt"] {
                println!("cargo:rustc-link-lib={}", lib);
            }
            println!("cargo:rerun-if-changed={}", dir.display());
            println!("cargo:rerun-if-env-changed=TOXCORE_LIB");
            // Let the caller point at libsodium via SODIUM_LIB (e.g. vcpkg).
            if let Ok(sodium) = env::var("SODIUM_LIB") {
                println!("cargo:rustc-link-search=native={}", sodium);
                println!("cargo:rustc-link-lib=static=sodium");
            }
        }
        None => {
            // No lib yet: still let the crate compile for `cargo check` purposes,
            // link failures will surface when actually building the binary.
            eprintln!(
                "warning: c-toxcore library not found. Looked in: {}",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!("cargo:rerun-if-env-changed=TOXCORE_LIB");
        }
    }

    // Make sure nothing stale gets cached across env changes.
    println!("cargo:rerun-if-changed=build.rs");
    let _ = Path::new(".").exists(); // no-op to keep clippy quiet
}
