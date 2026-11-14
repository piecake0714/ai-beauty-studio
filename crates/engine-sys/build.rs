//! Links against the prebuilt C++ engine.
//!
//! The engine is built and installed by CMake before cargo runs (see
//! README: "Build"). By default it is expected in `<repo>/engine/dist`;
//! set `AIBS_ENGINE_DIR` to use another install prefix.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=AIBS_ENGINE_DIR");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let engine_dir = env::var("AIBS_ENGINE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest_dir.join("..").join("..").join("engine").join("dist"));

    let lib_dir = engine_dir.join("lib");
    let bin_dir = engine_dir.join("bin");
    if !lib_dir.is_dir() {
        panic!(
            "C++ engine not found at {}. Build and install it with CMake first \
             (see README), or set AIBS_ENGINE_DIR to its install prefix.",
            engine_dir.display()
        );
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=aibs_engine");

    // On Windows the DLL must sit next to every executable that loads it:
    // copy it beside the app binary and beside test binaries.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let dll = bin_dir.join("aibs_engine.dll");
        println!("cargo:rerun-if-changed={}", dll.display());
        if !dll.is_file() {
            panic!("engine DLL not found at {}", dll.display());
        }
        // OUT_DIR = <target>/<profile>/build/<pkg>-<hash>/out
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let profile_dir = out_dir
            .ancestors()
            .nth(3)
            .expect("unexpected OUT_DIR layout")
            .to_path_buf();
        for dir in [profile_dir.clone(), profile_dir.join("deps")] {
            fs::create_dir_all(&dir).unwrap();
            fs::copy(&dll, dir.join("aibs_engine.dll")).unwrap();
        }
    }
}
