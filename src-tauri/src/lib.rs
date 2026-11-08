//! Easy Uninstaller backend. Each area of the app gets its own module and
//! exposes Tauri commands that the UI calls through `src/lib/api.ts`.

mod apps;
mod disk;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            apps::list_installed_apps,
            disk::get_system_disk
        ])
        .run(tauri::generate_context!())
        .expect("error while running Easy Uninstaller");
}
