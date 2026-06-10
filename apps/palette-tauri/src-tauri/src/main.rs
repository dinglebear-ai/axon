#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(err) = axon_palette_tauri_lib::run() {
        eprintln!("axon palette: fatal error: {err}");
        std::process::exit(1);
    }
}
