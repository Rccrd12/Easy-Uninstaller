//! Easy Uninstaller backend. Each area of the app gets its own module and
//! exposes Tauri commands that the UI calls through `src/lib/api.ts`.

mod apps;
mod disk;
mod files;
mod quarantine;
mod restore_point;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(files::LastScan::default())
        .setup(|app| quarantine::init(app.handle()))
        .invoke_handler(tauri::generate_handler![
            apps::list_installed_apps,
            disk::get_system_disk,
            files::scan_files,
            quarantine::quarantine_file_groups,
            quarantine::list_quarantine,
            quarantine::restore_quarantine,
            quarantine::delete_quarantine,
            restore_point::create_restore_point
        ])
        .run(tauri::generate_context!())
        .expect("error while running Easy Uninstaller");
}
