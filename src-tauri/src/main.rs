// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--sync") {
        std::process::exit(harpy_launcher_lib::run_headless_sync(&args));
    }
    if args.iter().any(|arg| arg == "--forget") {
        std::process::exit(harpy_launcher_lib::forget_launcher());
    }
    harpy_launcher_lib::run();
}
