// deprecated???: not part of the build. Cargo's default binary entrypoint is src/main.rs
// (this crate has no [[bin]] override in Cargo.toml pointing here), so this root-level copy
// is never compiled. Appears to be a leftover from before the src-tauri/src/ layout was
// introduced. Safe to delete after confirming via `cargo build -v` that it's unreferenced.
// See TODOS.md.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    app_lib::run();
}
