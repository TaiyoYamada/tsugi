// Without this a console window opens next to the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tsugi_desktop_lib::run()
}
