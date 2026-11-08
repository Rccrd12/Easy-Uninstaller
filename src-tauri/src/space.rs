//! Space map: how the system drive is used, measured folder by folder.
//!
//! Read only: this module never changes or removes anything. It measures the
//! folders of the user profile (one block per top-level folder), app data,
//! temporary files and the Recycle Bin. Installed apps come from `apps.rs`
//! through the UI, which already keeps that list, and whatever is left of the
//! used space is shown as "Windows and other".

use crate::disk;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::ipc::Channel;

/// Send a progress update every this many files.
const PROGRESS_EVERY: u64 = 2_000;

/// Mirrors the folder categories of `MapCategory` in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FolderCategory {
    /// App settings and data (`AppData`).
    Apps,
    Personal,
    Reclaimable,
}

/// Mirrors `SpaceFolder` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpaceFolder {
    pub id: String,
    /// i18n key for known folders; `None` when `label` (the folder name) is shown instead.
    pub label_key: Option<String>,
    pub label: Option<String>,
    /// Where it is, shown to the user; `None` when it spans several places.
    pub path: Option<String>,
    pub size_bytes: u64,
    pub category: FolderCategory,
}

/// Mirrors `SpaceUsage` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpaceUsage {
    pub mount_point: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// Biggest first; empty folders are left out.
    pub folders: Vec<SpaceFolder>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpaceProgress {
    pub files_checked: u64,
}

/// Something to measure: one block on the map.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    id: String,
    label_key: Option<String>,
    label: Option<String>,
    category: FolderCategory,
    /// Summed together (temporary files live in several places).
    roots: Vec<PathBuf>,
    /// Folders inside `roots` measured as a block of their own, or by `apps.rs`.
    skip: Vec<PathBuf>,
}

impl Target {
    fn keyed(id: &str, key: &str, category: FolderCategory, root: PathBuf) -> Self {
        Target {
            id: id.into(),
            label_key: Some(format!("map.blocks.{key}")),
            label: None,
            category,
            roots: vec![root],
            skip: Vec::new(),
        }
    }
}

#[tauri::command]
pub async fn scan_space(on_progress: Channel<SpaceProgress>) -> Result<SpaceUsage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let disk = disk::get_system_disk()?;
        let counter = Counter::new(move |files_checked| {
            // The window may be gone; the result is still returned.
            let _ = on_progress.send(SpaceProgress { files_checked });
        });
        let mut folders = match platform::profile_dir() {
            Some(profile) => {
                let caches = crate::files::cache_dirs();
                let local = platform::local_app_data();
                measure_all(
                    profile_targets(&profile, &caches, local.as_deref()),
                    &counter,
                )
            }
            None => Vec::new(),
        };
        if let Some(bytes) = platform::recycle_bin_bytes(&disk.mount_point) {
            folders.push(SpaceFolder {
                id: "recycle-bin".into(),
                label_key: Some("map.blocks.recycleBin".into()),
                label: None,
                path: None,
                size_bytes: bytes,
                category: FolderCategory::Reclaimable,
            });
        }
        folders.retain(|f| f.size_bytes > 0);
        folders.sort_by_key(|f| std::cmp::Reverse(f.size_bytes));
        Ok(SpaceUsage {
            mount_point: disk.mount_point,
            total_bytes: disk.total_bytes,
            free_bytes: disk.free_bytes,
            folders,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Known top-level folders of the user profile: (folder name, id and i18n key).
const KNOWN_FOLDERS: [(&str, &str); 6] = [
    ("desktop", "desktop"),
    ("documents", "documents"),
    ("downloads", "downloads"),
    ("pictures", "pictures"),
    ("videos", "videos"),
    ("music", "music"),
];

/// One target per top-level folder of `profile`, plus temporary files and caches.
/// `AppData` is measured without the caches (their own block) and without
/// `Local\Programs` and `Local\Packages`, where `apps.rs` finds installed apps.
fn profile_targets(profile: &Path, caches: &[PathBuf], local: Option<&Path>) -> Vec<Target> {
    let caches = dedup_dirs(caches);
    let mut targets = Vec::new();
    let Ok(entries) = fs::read_dir(profile) else {
        return targets;
    };
    for entry in entries.flatten() {
        // Junctions like "My Documents" are links: `is_dir` is false for them.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_lowercase();
        let path = entry.path();
        let mut target = if lower == "appdata" {
            let mut t = Target::keyed("appdata", "appData", FolderCategory::Apps, path.clone());
            t.skip.extend(caches.iter().cloned());
            if let Some(local) = local {
                t.skip.push(local.join("Programs"));
                t.skip.push(local.join("Packages"));
            }
            t
        } else if let Some((_, key)) = KNOWN_FOLDERS.iter().find(|(n, _)| *n == lower) {
            Target::keyed(key, key, FolderCategory::Personal, path.clone())
        } else if lower.starts_with("onedrive") {
            let mut t = Target::keyed(
                &format!("folder:{lower}"),
                "oneDrive",
                FolderCategory::Personal,
                path.clone(),
            );
            // "OneDrive - Company" keeps its name so two OneDrive folders can be told apart.
            if lower != "onedrive" {
                t.label_key = None;
                t.label = Some(name.clone());
            }
            t
        } else {
            Target {
                id: format!("folder:{lower}"),
                label_key: None,
                label: Some(name.clone()),
                category: FolderCategory::Personal,
                roots: vec![path.clone()],
                skip: Vec::new(),
            }
        };
        // Caches that live in this folder (e.g. Temp in AppData) are counted once, as caches.
        for cache in &caches {
            if cache.starts_with(&path) && !target.skip.contains(cache) {
                target.skip.push(cache.clone());
            }
        }
        targets.push(target);
    }
    targets.sort_by(|a, b| a.id.cmp(&b.id));
    if !caches.is_empty() {
        targets.push(Target {
            id: "caches".into(),
            label_key: Some("map.blocks.caches".into()),
            label: None,
            category: FolderCategory::Reclaimable,
            roots: caches,
            skip: Vec::new(),
        });
    }
    targets
}

/// Measures every target, each on its own thread (the disk is the limit, not the CPU).
fn measure_all(targets: Vec<Target>, counter: &Counter) -> Vec<SpaceFolder> {
    let sizes: Vec<u64> = std::thread::scope(|s| {
        let handles: Vec<_> = targets
            .iter()
            .map(|t| {
                s.spawn(move || {
                    let skip: Vec<String> = t.skip.iter().map(|p| norm(p)).collect();
                    t.roots.iter().map(|r| measure(r, &skip, counter)).sum()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or(0)).collect()
    });
    targets
        .into_iter()
        .zip(sizes)
        .map(|(t, size_bytes)| SpaceFolder {
            path: (t.roots.len() == 1).then(|| t.roots[0].to_string_lossy().into_owned()),
            size_bytes,
            id: t.id,
            label_key: t.label_key,
            label: t.label,
            category: t.category,
        })
        .collect()
}

/// Lowercase path with `/` as the only separator and none at the end: Windows paths
/// ignore case and accept both separators (`C:\Users\a/AppData` is a valid path).
fn norm(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

/// Total size of the files under `root`. Does not follow links or junctions, does
/// not enter folders in `skip` (normalized with [`norm`]), leaves out cloud-only
/// files (they use no disk space) and skips what it cannot read.
fn measure(root: &Path, skip: &[String], counter: &Counter) -> u64 {
    let mut total = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                let path = entry.path();
                if skip.is_empty() || !skip.contains(&norm(&path)) {
                    stack.push(path);
                }
            } else if kind.is_file() {
                if let Ok(meta) = entry.metadata() {
                    if !platform::cloud_only(&meta) {
                        total += meta.len();
                    }
                }
                counter.tick();
            }
        }
    }
    total
}

/// Keeps only the outermost folders, so nothing is counted twice.
fn dedup_dirs(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        if !out.iter().any(|d| dir.starts_with(d)) {
            out.retain(|d| !d.starts_with(dir));
            out.push(dir.clone());
        }
    }
    out
}

/// Counts files across threads and reports every [`PROGRESS_EVERY`].
struct Counter {
    files: AtomicU64,
    report: Box<dyn Fn(u64) + Send + Sync>,
}

impl Counter {
    fn new(report: impl Fn(u64) + Send + Sync + 'static) -> Self {
        Counter {
            files: AtomicU64::new(0),
            report: Box::new(report),
        }
    }

    fn tick(&self) {
        let n = self.files.fetch_add(1, Ordering::Relaxed) + 1;
        if n % PROGRESS_EVERY == 0 {
            (self.report)(n);
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::fs::Metadata;
    use std::os::windows::fs::MetadataExt;
    use std::path::PathBuf;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x40000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x400000;

    /// OneDrive "online only" files use no disk space.
    pub fn cloud_only(meta: &Metadata) -> bool {
        let cloud = FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
        meta.file_attributes() & cloud != 0
    }

    fn env(name: &str) -> Option<PathBuf> {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    pub fn profile_dir() -> Option<PathBuf> {
        env("USERPROFILE").filter(|p| p.is_dir())
    }

    pub fn local_app_data() -> Option<PathBuf> {
        env("LOCALAPPDATA")
    }

    /// Size of the Recycle Bin on `drive` (e.g. `C:`), as Explorer reports it.
    pub fn recycle_bin_bytes(drive: &str) -> Option<u64> {
        let root: Vec<u16> = format!("{drive}\\")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut info = SHQUERYRBINFO {
            cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call,
        // and `info` is a properly sized, writable SHQUERYRBINFO.
        unsafe { SHQueryRecycleBinW(PCWSTR(root.as_ptr()), &mut info) }.ok()?;
        u64::try_from(info.i64Size).ok()
    }
}

/// Easy Uninstaller targets Windows; elsewhere there is nothing to measure.
#[cfg(not(windows))]
mod platform {
    use std::fs::Metadata;
    use std::path::PathBuf;

    pub fn cloud_only(_meta: &Metadata) -> bool {
        false
    }

    pub fn profile_dir() -> Option<PathBuf> {
        None
    }

    pub fn local_app_data() -> Option<PathBuf> {
        None
    }

    pub fn recycle_bin_bytes(_drive: &str) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty folder under the system temp dir, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "easy-uninstaller-space-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn file(&self, rel: &str, size: usize) {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, vec![0u8; size]).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn silent() -> Counter {
        Counter::new(|_| {})
    }

    #[test]
    fn measure_sums_nested_files_and_skips_folders() {
        let tmp = TempDir::new("measure");
        tmp.file("a.bin", 100);
        tmp.file("sub/deep/b.bin", 20);
        tmp.file("Skipped/c.bin", 7);
        let counter = silent();
        assert_eq!(measure(&tmp.0, &[], &counter), 127);
        // Case and a trailing separator do not matter.
        let skip = vec![norm(&tmp.0.join("skipped/"))];
        assert_eq!(measure(&tmp.0, &skip, &counter), 120);
        assert_eq!(
            norm(Path::new("C:\\Users\\A/AppData\\")),
            "c:/users/a/appdata"
        );
        assert_eq!(measure(&tmp.0.join("missing"), &[], &counter), 0);
        assert_eq!(counter.files.load(Ordering::Relaxed), 5);
    }

    #[cfg(unix)]
    #[test]
    fn measure_does_not_follow_links() {
        let tmp = TempDir::new("links");
        let outside = TempDir::new("links-outside");
        outside.file("big.bin", 500);
        tmp.file("own.bin", 10);
        std::os::unix::fs::symlink(&outside.0, tmp.0.join("link")).unwrap();
        assert_eq!(measure(&tmp.0, &[], &silent()), 10);
    }

    #[test]
    fn counter_reports_every_few_thousand_files() {
        let seen = std::sync::Arc::new(AtomicU64::new(0));
        let s = seen.clone();
        let counter = Counter::new(move |n| s.store(n, Ordering::Relaxed));
        for _ in 0..PROGRESS_EVERY * 2 + 5 {
            counter.tick();
        }
        assert_eq!(seen.load(Ordering::Relaxed), PROGRESS_EVERY * 2);
    }

    #[test]
    fn dedup_keeps_outermost_folders() {
        let dirs = [
            PathBuf::from("/a/b"),
            PathBuf::from("/a"),
            PathBuf::from("/c"),
        ];
        assert_eq!(
            dedup_dirs(&dirs),
            vec![PathBuf::from("/a"), PathBuf::from("/c")]
        );
    }

    #[test]
    fn profile_folders_become_blocks_without_counting_twice() {
        let p = TempDir::new("profile");
        p.file("Documents/report.pdf", 300);
        p.file("Pictures/cat.jpg", 200);
        p.file("Games/save.dat", 50);
        p.file("OneDrive - Acme/plan.docx", 40);
        p.file("AppData/Roaming/App/settings.json", 30);
        p.file("AppData/Local/Temp/old.tmp", 1_000);
        p.file("AppData/Local/Programs/Editor/editor.exe", 5_000);
        p.file("AppData/Local/Packages/Store.App/data.bin", 4_000);
        p.file("NTUSER.DAT", 9);
        let local = p.0.join("AppData/Local");
        let caches = vec![local.join("Temp"), local.join("Temp/inner")];

        let targets = profile_targets(&p.0, &caches, Some(&local));
        let ids: Vec<&str> = targets.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "appdata",
                "documents",
                "folder:games",
                "folder:onedrive - acme",
                "pictures",
                "caches"
            ]
        );
        let caches_target = targets.last().unwrap();
        assert_eq!(caches_target.roots, vec![local.join("Temp")]);

        let folders = measure_all(targets, &silent());
        let size = |id: &str| folders.iter().find(|f| f.id == id).unwrap().size_bytes;
        assert_eq!(size("appdata"), 30);
        assert_eq!(size("caches"), 1_000);
        assert_eq!(size("documents"), 300);
        assert_eq!(size("folder:games"), 50);

        let docs = folders.iter().find(|f| f.id == "documents").unwrap();
        assert_eq!(docs.label_key.as_deref(), Some("map.blocks.documents"));
        assert_eq!(docs.category, FolderCategory::Personal);
        let games = folders.iter().find(|f| f.id == "folder:games").unwrap();
        assert_eq!(
            (games.label_key.as_deref(), games.label.as_deref()),
            (None, Some("Games"))
        );
        let work = folders
            .iter()
            .find(|f| f.id == "folder:onedrive - acme")
            .unwrap();
        assert_eq!(work.label.as_deref(), Some("OneDrive - Acme"));
        let app_data = folders.iter().find(|f| f.id == "appdata").unwrap();
        assert_eq!(app_data.category, FolderCategory::Apps);
    }

    #[test]
    fn serializes_like_the_typescript_type() {
        let usage = SpaceUsage {
            mount_point: "C:".into(),
            total_bytes: 10,
            free_bytes: 4,
            folders: vec![SpaceFolder {
                id: "caches".into(),
                label_key: Some("map.blocks.caches".into()),
                label: None,
                path: None,
                size_bytes: 3,
                category: FolderCategory::Reclaimable,
            }],
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("\"mountPoint\":\"C:\""));
        assert!(json.contains("\"freeBytes\":4"));
        assert!(json.contains("\"labelKey\":\"map.blocks.caches\""));
        assert!(json.contains("\"sizeBytes\":3"));
        assert!(json.contains("\"category\":\"reclaimable\""));
    }
}
