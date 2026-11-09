//! Removing what the file scan found, group by group.
//!
//! Most groups go to the quarantine (`quarantine.rs`), so they can be restored.
//! The groups in [`DELETE_NOW_GROUPS`] hold only files nobody needs back:
//! temporary files, caches and thumbnails (Windows and apps recreate them), crash
//! reports, and Recycle Bin items deleted over 30 days ago. Keeping them in
//! quarantine would give no space back, so they are deleted right away. This is the one place
//! where Easy Uninstaller deletes files directly (the owner's decision), and only
//! files the scan listed in those groups, still unchanged since the scan.

use crate::files::{to_camel, LastScan, ScannedFile};
use crate::history::{self, Kind};
use crate::quarantine::{unchanged_since_scan, Item, MoveReport, Quarantine, Title};
use std::fs;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// Groups deleted right away instead of going to the quarantine.
/// Mirrored by `DELETE_NOW_GROUPS` in `src/lib/api.ts`.
pub const DELETE_NOW_GROUPS: [&str; 4] = ["temp", "recycle-bin", "crash-reports", "thumbnails"];

pub fn deletes_now(group_id: &str) -> bool {
    DELETE_NOW_GROUPS.contains(&group_id)
}

/// The i18n key of a file group's title, as shown in Quarantine and History.
pub fn group_title(group_id: &str) -> Title {
    Title::Key(format!("files.groups.{}.title", to_camel(group_id)))
}

/// Removes `files` of the group `group_id`: deleted right away for the groups in
/// [`DELETE_NOW_GROUPS`], moved to a new quarantine entry otherwise.
pub fn remove(
    quarantine: &Quarantine,
    group_id: &str,
    files: &[ScannedFile],
    now: u64,
    report: &mut MoveReport,
) -> io::Result<()> {
    if deletes_now(group_id) {
        delete(files, report);
        Ok(())
    } else {
        let items: Vec<Item> = files.iter().map(Item::from).collect();
        quarantine
            .add(group_title(group_id), &items, now, false, report)
            .map(|_| ())
    }
}

/// Deletes the files that are still as the scan saw them. Files in use, changed
/// or gone are left alone and counted as skipped.
fn delete(files: &[ScannedFile], report: &mut MoveReport) {
    for file in files {
        if unchanged_since_scan(&file.path, file.size, file.modified)
            && fs::remove_file(&file.path).is_ok()
        {
            report.deleted_count += 1;
            report.deleted_bytes += file.size;
            report.freed_bytes += file.size;
        } else {
            report.skipped_count += 1;
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Removes whole groups of the last file scan (used by Quick clean, `quick`, and
/// by the Files screen). Only group ids come from the UI: the files are the ones
/// the scan itself found.
#[tauri::command]
pub async fn remove_file_groups(
    app: AppHandle,
    group_ids: Vec<String>,
    quick: Option<bool>,
) -> Result<MoveReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let quarantine = app.state::<Quarantine>();
        let last = app.state::<LastScan>();
        let mut report = MoveReport::default();
        let mut names = Vec::new();
        let mut result = Ok(());
        for id in group_ids {
            let Some(files) = last.group(&id) else {
                continue;
            };
            if files.is_empty() {
                continue;
            }
            names.push(group_title(&id));
            result = remove(&quarantine, &id, &files, now_secs(), &mut report);
            if result.is_err() {
                break;
            }
        }
        // What was removed before a failure is recorded too.
        let kind = if quick == Some(true) {
            Kind::QuickClean
        } else {
            Kind::Files
        };
        history::record_move(&app, kind, names, &report);
        result.map_err(|e| e.to_string())?;
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Sandbox(PathBuf);

    impl Sandbox {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("easy-uninstaller-c-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Sandbox(dir)
        }

        fn file(&self, rel: &str, content: &str) -> ScannedFile {
            let path = self.0.join("home").join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            let meta = fs::metadata(&path).unwrap();
            ScannedFile {
                path,
                size: meta.len(),
                modified: meta.modified().unwrap(),
            }
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn deletes_temp_and_quarantines_the_rest() {
        let s = Sandbox::new("mixed");
        let q = Quarantine::new(s.0.join("q"));
        let tmp = s.file("Temp/a.tmp", "aaaa");
        let changed = s.file("Temp/b.tmp", "bb");
        fs::write(&changed.path, "changed").unwrap();
        let setup = s.file("Downloads/setup.exe", "123456");
        let mut report = MoveReport::default();

        remove(
            &q,
            "temp",
            &[tmp.clone(), changed.clone()],
            NOW,
            &mut report,
        )
        .unwrap();
        remove(
            &q,
            "installers",
            std::slice::from_ref(&setup),
            NOW,
            &mut report,
        )
        .unwrap();

        assert!(!tmp.path.exists(), "deleted");
        assert!(changed.path.exists(), "changed since the scan: left alone");
        assert!(!setup.path.exists());
        assert_eq!((report.deleted_count, report.deleted_bytes), (1, 4));
        assert_eq!((report.moved_count, report.moved_bytes), (1, 6));
        assert_eq!(report.skipped_count, 1);
        // Same drive: only the deleted file gave space back now.
        assert_eq!(report.freed_bytes, 4);

        let entries = q.list();
        assert_eq!(entries.len(), 1, "temp files never reach the quarantine");
        assert_eq!(
            entries[0].title_key.as_deref(),
            Some("files.groups.installers.title")
        );
        assert_eq!(report.entry_ids, [entries[0].id.clone()]);
    }

    #[test]
    fn only_listed_groups_are_deleted() {
        for id in ["temp", "recycle-bin", "crash-reports", "thumbnails"] {
            assert!(deletes_now(id), "{id}");
        }
        for id in [
            "installers",
            "old-downloads",
            "duplicates",
            "large-files",
            "unknown-folders",
        ] {
            assert!(!deletes_now(id), "{id}");
        }
    }
}
