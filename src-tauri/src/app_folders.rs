//! Moves one big app folder (from the "unknown folders" group) into quarantine,
//! when the user picks it on the Files screen.
//!
//! The UI sends a path, so it is checked again here rather than trusted: only a
//! real folder right inside LOCALAPPDATA, APPDATA or ProgramData, never one of
//! Windows' own (see `hidden` in `known_folders.json`) and never the folder that
//! holds the quarantine itself. Folders outside the user's profile (ProgramData)
//! go through the elevated helper, so Windows asks for permission.

use crate::elevated::{self, Job};
use crate::explain::KnownFolders;
use crate::leftovers::inside;
use crate::quarantine::{self, now_secs, Item, MoveReport, Quarantine, Target, Title};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Why a folder cannot be moved.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not right inside AppData or ProgramData.
    NotAnAppFolder,
    /// Gone, a link or a junction.
    NotARealFolder,
    /// Windows' own data, or already handled by another group.
    Protected,
    /// Holds Easy Uninstaller's own data (and the quarantine).
    OwnData,
}

/// Checks that `path` is a folder the "unknown folders" group can list.
pub fn check(
    path: &Path,
    roots: &[PathBuf],
    known: &KnownFolders,
    own_data: &Path,
) -> Result<(), Refusal> {
    let parent = path.parent().ok_or(Refusal::NotAnAppFolder)?;
    let is_child =
        path.file_name().is_some() && roots.iter().any(|r| inside(parent, r) && inside(r, parent));
    if !is_child {
        return Err(Refusal::NotAnAppFolder);
    }
    let meta = fs::symlink_metadata(path).map_err(|_| Refusal::NotARealFolder)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(Refusal::NotARealFolder);
    }
    if known.lookup(path).is_some_and(|f| f.hidden) {
        return Err(Refusal::Protected);
    }
    if inside(own_data, path) {
        return Err(Refusal::OwnData);
    }
    Ok(())
}

fn size_of(path: &Path) -> u64 {
    let mut size = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() && !kind.is_symlink() {
                stack.push(entry.path());
            } else if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    size += meta.len();
                }
            }
        }
    }
    size
}

fn env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Moves one folder of the "unknown folders" group into quarantine, as one entry
/// titled with the folder's name. A folder in use is left in place (skipped).
#[tauri::command]
pub async fn quarantine_app_folder(app: AppHandle, path: String) -> Result<MoveReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(path);
        let roots: Vec<PathBuf> = ["LOCALAPPDATA", "APPDATA", "ProgramData"]
            .iter()
            .filter_map(|v| env(v))
            .collect();
        let own_data = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
        check(&path, &roots, &KnownFolders::from_env(), &own_data)
            .map_err(|r| format!("refused: {r:?}"))?;

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let item = Item {
            target: Target::Folder(path.clone()),
            size: size_of(&path),
            modified: None,
        };
        let title = Title::Text(name);
        let needs_admin = env("USERPROFILE").map_or(true, |home| !inside(&path, &home));
        let mut report = MoveReport::default();
        if needs_admin {
            let job = Job::Add {
                root: quarantine::root_of(&app),
                groups: vec![(title, vec![item])],
            };
            match elevated::run(&app, &job)? {
                Some(result) => report.merge(result.moved.unwrap_or_default()),
                None => report.declined_count += 1,
            }
        } else {
            app.state::<Quarantine>()
                .add(title, &[item], now_secs(), false, &mut report)
                .map_err(|e| e.to_string())?;
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "easy-uninstaller-appfolders-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn only_real_app_folders_can_move() {
        let t = TempDir::new("check");
        let local = t.0.join("Local");
        for dir in [
            "Local/BigApp/sub",
            "Local/Microsoft",
            "Local/com.easy/Quarantine",
            "Other/App",
        ] {
            fs::create_dir_all(t.0.join(dir)).unwrap();
        }
        fs::write(t.0.join("Local/BigApp/sub/a.bin"), [1u8; 300]).unwrap();
        fs::write(t.0.join("Local/file.txt"), b"x").unwrap();
        let known = KnownFolders::new(|n| (n == "LOCALAPPDATA").then(|| local.clone()));
        let roots = vec![local.clone()];
        let own = local.join("com.easy");
        let check = |p: PathBuf| check(&p, &roots, &known, &own);

        assert_eq!(check(local.join("BigApp")), Ok(()));
        assert_eq!(size_of(&local.join("BigApp")), 300);
        assert_eq!(
            check(local.join("BigApp/sub")),
            Err(Refusal::NotAnAppFolder),
            "only direct children"
        );
        assert_eq!(
            check(local.clone()),
            Err(Refusal::NotAnAppFolder),
            "never the root itself"
        );
        assert_eq!(check(t.0.join("Other/App")), Err(Refusal::NotAnAppFolder));
        assert_eq!(check(local.join("Missing")), Err(Refusal::NotARealFolder));
        assert_eq!(check(local.join("file.txt")), Err(Refusal::NotARealFolder));
        assert_eq!(check(local.join("Microsoft")), Err(Refusal::Protected));
        assert_eq!(check(own.clone()), Err(Refusal::OwnData));
        assert_eq!(
            check(local.join("BigApp/../Microsoft")),
            Err(Refusal::NotAnAppFolder)
        );
    }

    #[cfg(unix)]
    #[test]
    fn links_are_refused() {
        let t = TempDir::new("link");
        let local = t.0.join("Local");
        fs::create_dir_all(t.0.join("Elsewhere")).unwrap();
        fs::create_dir_all(&local).unwrap();
        std::os::unix::fs::symlink(t.0.join("Elsewhere"), local.join("Link")).unwrap();
        let known = KnownFolders::default();
        assert_eq!(
            check(
                &local.join("Link"),
                std::slice::from_ref(&local),
                &known,
                &t.0.join("own")
            ),
            Err(Refusal::NotARealFolder)
        );
    }
}
