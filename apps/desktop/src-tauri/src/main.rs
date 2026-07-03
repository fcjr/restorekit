#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    restorekit_desktop_lib::run()
}
