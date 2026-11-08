//! Easy Uninstaller backend. Each area of the app gets its own module and
//! exposes Tauri commands that the UI calls through `src/lib/api.ts`.

mod app_folders;
mod apps;
mod disk;
mod elevated;
mod explain;
mod files;
mod leftovers;
mod quarantine;
mod regkey;
mod restore_point;
mod space;
mod uninstall;

/// When started as the elevated helper (see `elevated.rs`), does its job and
/// returns the exit code; `None` means start the app normally.
pub fn run_elevated_job() -> Option<i32> {
    elevated::run_from_args()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(files::LastScan::default())
        .manage(leftovers::LastLeftovers::default())
        .manage(uninstall::Pending::default())
        .setup(|app| quarantine::init(app.handle()))
        .invoke_handler(tauri::generate_handler![
            app_folders::quarantine_app_folder,
            apps::list_installed_apps,
            disk::get_system_disk,
            files::scan_files,
            leftovers::list_leftovers,
            leftovers::quarantine_leftovers,
            quarantine::quarantine_file_groups,
            quarantine::list_quarantine,
            quarantine::restore_quarantine,
            quarantine::delete_quarantine,
            restore_point::create_restore_point,
            space::scan_space,
            uninstall::uninstall_app,
            uninstall::is_app_installed,
            uninstall::find_app_leftovers
        ])
        .run(tauri::generate_context!())
        .expect("error while running Easy Uninstaller");
}
