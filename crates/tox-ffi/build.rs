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
        Some(workspace.join("build/c-toxcore/Release")),
        Some(workspace.join("build/c-toxcore/Debug")),
        Some(workspace.join("build/c-toxcore")),
        Some(workspace.join("third_party/c-toxcore/build")),
        Some(workspace.join("third_party/c-toxcore/_build")),
    ]
    .into_iter()
    .flatten()
    .collect();

    // MSVC static builds are named toxcore_static.lib / toxcore.lib;
    // MinGW style: libtoxcore.a / libtoxcore_static.a.
    const LIB_NAMES: &[&str] = &[
        "toxcore_static.lib",
        "toxcore.lib",
        "libtoxcore_static.a",
        "libtoxcore.a",
    ];

    let mut found: Option<(PathBuf, &'static str)> = None;
    for dir in &candidates {
        for name in LIB_NAMES {
            if dir.join(name).exists() {
                found = Some((dir.clone(), name));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }

    match found {
        Some((dir, lib_name)) => {
            println!("cargo:rustc-link-search=native={}", dir.display());
            let stem = lib_name
                .trim_start_matches("lib")
                .trim_end_matches(".lib")
                .trim_end_matches(".a");
            println!("cargo:rustc-link-lib=static={}", stem);
            // Windows system deps required by c-toxcore (+ pthreads4w via vcpkg).
            for lib in [
                "ws2_32",
                "iphlpapi",
                "advapi32",
                "user32",
                "shell32",
                "bcrypt",
                "pthreadVC3",
            ] {
                println!("cargo:rustc-link-lib={}", lib);
            }
            println!("cargo:rerun-if-changed={}", dir.display());
            println!("cargo:rerun-if-env-changed=TOXCORE_LIB");

            // libsodium + pthreads4w: search vcpkg dirs (global install via
            // SODIUM_LIB, or the per-project vcpkg_installed used by the
            // c-toxcore CMake build).
            let mut dep_dirs: Vec<PathBuf> = vec![
                env::var("SODIUM_LIB").ok().map(PathBuf::from),
                Some(workspace.join(
                    "build/c-toxcore/vcpkg_installed/x64-windows/lib",
                )),
                Some(workspace.join(
                    "build/c-toxcore/vcpkg_installed/x64-windows/debug/lib",
                )),
            ]
            .into_iter()
            .flatten()
            .collect();
            dep_dirs.dedup();
            for d in &dep_dirs {
                if d.exists() {
                    println!("cargo:rustc-link-search=native={}", d.display());
                }
            }
            // Only link libsodium/pthreads if the libs are actually present.
            let has_sodium = dep_dirs.iter().any(|d| d.join("libsodium.lib").exists());
            let has_pthread = dep_dirs.iter().any(|d| d.join("pthreadVC3.lib").exists());
            if has_sodium {
                println!("cargo:rustc-link-lib=static=libsodium");
            }
            if has_pthread {
                println!("cargo:rustc-link-lib=pthreadVC3");
            }
            println!("cargo:rerun-if-env-changed=SODIUM_LIB");
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
