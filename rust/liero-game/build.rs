// Build-time setup for two targets:
// - Emscripten: link options for the hash run's wasm build
//   (src/bin/liero-hashrun.rs), which CI runs with Bun as well as natively:
//   the files it reads and writes are the disk's, through Node's file
//   system, and every function checks for a stack overflow, as the web build
//   CI makes does (LIERO_STACK_CHECK).
// - wasm32-unknown-unknown, the web build (src/web): the bundled TC, listed
//   for `include_bytes!`, since the browser has no disk to read it from.
use std::fmt::Write;
use std::path::{Path, PathBuf};

fn tc_files(dir: &Path, rel: &str, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("can't list TC/openliero").flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = format!("{rel}/{}", e.file_name().to_str().expect("a TC file name that isn't UTF-8"));
        if e.file_type().expect("can't read a TC entry").is_dir() {
            tc_files(&e.path(), &name, out);
        } else {
            out.push((name, e.path()));
        }
    }
}

fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if os == "emscripten" {
        println!("cargo:rustc-link-arg-bin=liero-hashrun=-sNODERAWFS=1");
        println!("cargo:rustc-link-arg-bin=liero-hashrun=-sSTACK_OVERFLOW_CHECK=2");
    }
    if arch == "wasm32" && os == "unknown" {
        let tc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../TC/openliero");
        println!("cargo:rerun-if-changed={}", tc.display());
        let mut files = Vec::new();
        tc_files(&tc, "TC/openliero", &mut files);
        let mut src = String::from("/// The bundled TC's files, by path from the game's root.\npub const TC_FILES: &[(&str, &[u8])] = &[\n");
        for (name, path) in &files {
            println!("cargo:rerun-if-changed={}", path.display());
            writeln!(src, "    ({name:?}, include_bytes!({:?})),", path.canonicalize().expect("a TC file").display()).unwrap();
        }
        src.push_str("];\n");
        let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("tc_files.rs");
        std::fs::write(out, src).expect("can't write tc_files.rs");
    }
}
