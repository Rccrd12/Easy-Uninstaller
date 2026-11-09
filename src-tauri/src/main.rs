// Hide the extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = easy_uninstaller_lib::run_elevated_job() {
        std::process::exit(code);
    }
    if let Some(code) = easy_uninstaller_lib::run_reminders_check() {
        std::process::exit(code);
    }
    easy_uninstaller_lib::run()
}
