//! Easy Uninstaller backend. Each area of the app gets its own module and
//! exposes Tauri commands that the UI calls through `src/lib/api.ts`.

mod ai;
mod app_cache;
mod app_folders;
mod apps;
mod cleanup;
mod disk;
mod elevated;
mod explain;
mod files;
mod history;
mod leftovers;
mod offload;
mod quarantine;
mod regkey;
mod reinstall;
mod reminders;
mod restore_point;
mod review;
mod search;
mod space;
mod startup;
mod tidy;
mod uninstall;
mod updater;

/// When started as the elevated helper (see `elevated.rs`), does its job and
/// returns the exit code; `None` means start the app normally.
pub fn run_elevated_job() -> Option<i32> {
    elevated::run_from_args()
}

/// When started by the reminders' scheduled task (see `reminders.rs`), checks whether
/// a notification is due and returns the exit code; `None` means start the app normally.
pub fn run_reminders_check() -> Option<i32> {
    reminders::run_from_args()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ai::Answers::default())
        .manage(app_cache::LastAppCaches::default())
        .manage(files::LastScan::default())
        .manage(leftovers::LastLeftovers::default())
        .manage(reinstall::Prepared::default())
        .manage(offload::LastOffload::default())
        .manage(search::LastSearch::default())
        .manage(tidy::Tidy::default())
        .manage(uninstall::Pending::default())
        .manage(updater::PendingUpdate::default())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            quarantine::init(app.handle())?;
            reminders::init(app.handle())?;
            history::init(app.handle())?;
            review::init(app.handle())
        })
        .invoke_handler(tauri::generate_handler![
            ai::ai_services,
            ai::ai_models,
            ai::ai_set_key,
            ai::ai_remove_key,
            ai::ai_test,
            ai::ai_preview,
            ai::ai_ask,
            ai::ai_saved_answers,
            ai::ai_stats,
            ai::ai_forget_answers,
            ai::ai_open_page,
            app_cache::list_app_caches,
            app_cache::clear_app_cache,
            app_folders::quarantine_app_folder,
            apps::list_installed_apps,
            cleanup::remove_file_groups,
            disk::get_system_disk,
            files::scan_files,
            history::get_history,
            history::clear_history,
            leftovers::list_leftovers,
            leftovers::quarantine_leftovers,
            offload::scan_offload,
            offload::current_offload,
            offload::make_online_only,
            offload::keep_on_this_pc,
            offload::move_to_drive,
            offload::move_back_files,
            offload::show_moved_file,
            quarantine::get_quarantine_options,
            quarantine::set_quarantine_settings,
            quarantine::list_quarantine,
            quarantine::restore_quarantine,
            quarantine::delete_quarantine,
            reminders::get_reminders,
            reminders::set_reminders,
            reminders::reminders_opened,
            reminders::preview_reminder,
            reinstall::list_uninstalled_apps,
            reinstall::remember_uninstalled,
            reinstall::forget_uninstalled,
            reinstall::reinstall_app,
            restore_point::create_restore_point,
            review::current_file_groups,
            review::review_file_group,
            review::quarantine_file_selection,
            review::list_exclusions,
            review::add_exclusions,
            review::remove_exclusions,
            review::file_thumbnail,
            review::open_scanned_file,
            review::show_scanned_file,
            search::search_understand,
            search::search_files,
            search::quarantine_found,
            space::scan_space,
            startup::list_startup_apps,
            startup::set_startup_apps,
            tidy::tidy_overview,
            tidy::tidy_preview,
            tidy::tidy_ask,
            tidy::tidy_by_type,
            tidy::tidy_apply,
            tidy::tidy_undo,
            uninstall::uninstall_app,
            uninstall::is_app_installed,
            uninstall::find_app_leftovers,
            updater::check_for_update,
            updater::install_update
        ])
        .build(tauri::generate_context!())
        .expect("error while building Easy Uninstaller")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                ai::on_exit();
            }
        });
}
