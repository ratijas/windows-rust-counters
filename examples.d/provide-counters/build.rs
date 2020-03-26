use path_slash::PathBufExt;
use std::path::PathBuf;
use std::env;

const MODULE_DEFINITION: &'static str = "./resources/exports.def";

fn main() {
    let module_definition = PathBuf::from(env::current_dir().unwrap())
        .join(PathBuf::from_slash(MODULE_DEFINITION))
        .to_string_lossy()
        .into_owned();
    cargo_emit::rerun_if_changed!(module_definition);
    cargo_emit::rustc_cdylib_link_arg!(format!("/DEF:{}", module_definition))
}
