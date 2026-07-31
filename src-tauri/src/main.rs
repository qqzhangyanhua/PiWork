#![cfg_attr(windows, windows_subsystem = "windows")]
#![allow(linker_messages)] // Xberg's bundled static Tesseract selects a release CRT in debug builds.

fn main() {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    if arguments.next().as_deref() == Some(std::ffi::OsStr::new("--document-runtime")) {
        let root = arguments.next().map(std::path::PathBuf::from);
        let code = root.map_or(2, piwork_lib::document_runtime::run_child_from_stdio);
        std::process::exit(code);
    }
    piwork_lib::run();
}
