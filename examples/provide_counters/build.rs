const MODULE_DEFINITION: &'static str = "./resources/exports.def";

fn main() {
    println!("cargo:rerun-if-changed={}", MODULE_DEFINITION);
    println!("cargo:rustc-cdylib-link-arg=/DEF:{}", MODULE_DEFINITION);
}
