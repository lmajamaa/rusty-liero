// Link options for the hash run's wasm build (src/bin/liero-hashrun.rs),
// which CI runs with Bun as well as natively: the files it reads and writes
// are the disk's, through Node's file system, and every function checks for
// a stack overflow, as the web build CI makes does (LIERO_STACK_CHECK).
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("emscripten") {
        println!("cargo:rustc-link-arg-bin=liero-hashrun=-sNODERAWFS=1");
        println!("cargo:rustc-link-arg-bin=liero-hashrun=-sSTACK_OVERFLOW_CHECK=2");
    }
}
