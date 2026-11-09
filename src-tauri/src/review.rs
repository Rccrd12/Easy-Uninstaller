//! Reviewing the files of a scan one by one, and what the user chose to hide
//! ("exclusions"): files, folders, whole file groups and leftovers. Hiding never
//! moves or deletes anything; the app only stops showing and suggesting it.
//!
//! Everything here works on the files the last scan found (`files::LastScan`):
//! the UI sends back paths, and only paths the scan found are ever moved, opened
//! or shown. What the UI sees is rebuilt from that list each time, leaving out
//! files that are gone or changed since the scan (moved, edited, restored under
//! a new name) and excluded ones, so every screen agrees after a move or an undo.
//!
//! Duplicates: the scan keeps each set of identical files, oldest first. The user
//! can choose which copy to keep, but at least one copy of every set always stays
//! where it is: a request to move all of them leaves the oldest in place.

use crate::cleanup;
use crate::files::{iso_date, FileGroup, FileItem, Kept, LastScan, ScannedFile, Seen, MAX_ITEMS};
use crate::history;
use crate::quarantine::{MoveReport, Quarantine};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const DUPLICATES: &str = "duplicates";
/// How many files of a group the review lists (largest first); the rest are counted.
const MAX_REVIEW_FILES: usize = 2000;
/// How many duplicate sets the review lists (most space first).
const MAX_REVIEW_SETS: usize = 1000;
const MAX_EXCLUSIONS: usize = 2000;
const EXCLUSIONS_FILE: &str = "exclusions.json";
/// File types that open in a viewer (photos, videos, documents). Anything else,
/// and above all programs and scripts, is never opened from the app.
const OPENABLE: [&str; 52] = [
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "heic", "heif", "tif", "tiff", "dng", "cr2", "nef",
    "arw", "mp4", "mov", "avi", "mkv", "wmv", "m4v", "webm", "3gp", "mts", "mp3", "wav", "flac",
    "m4a", "aac", "ogg", "wma", "pdf", "txt", "md", "rtf", "csv", "log", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "odt", "ods", "odp", "epub", "zip", "7z", "rar", "iso", "psd", "svg",
];

/// A file as the review lists it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFile {
    pub path: String,
    pub size_bytes: u64,
    /// `YYYY-MM-DD`: the later of last opened and last changed.
    pub last_used: String,
    /// `YYYY-MM-DD`.
    pub modified: String,
    /// A photo, video or document that "Open" may show; never a program.
    pub can_open: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroupReview {
    pub group_id: String,
    /// Files that can be moved: for duplicates, the extra copies.
    pub total_count: u64,
    pub total_bytes: u64,
    /// Largest first, at most [`MAX_REVIEW_FILES`]. Empty for duplicates.
    pub files: Vec<ReviewFile>,
    /// Duplicates only: sets of identical files, each oldest first (the copy
    /// kept by default), most space first, at most [`MAX_REVIEW_SETS`].
    pub duplicate_sets: Vec<Vec<ReviewFile>>,
}

/// Some files of one group, picked by the user.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSelection {
    pub group_id: String,
    pub paths: Vec<String>,
}

/// What a hidden item is.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HiddenKind {
    #[default]
    File,
    /// A folder and everything inside it.
    Folder,
    /// A whole file group, by id (e.g. `large-files`).
    Group,
    /// A leftover of an uninstalled app, by the leftovers screen's id.
    Leftover,
}

/// Something never to show again.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Exclusion {
    /// A path for files and folders, an id for groups and leftovers.
    pub path: String,
    #[serde(default)]
    pub kind: HiddenKind,
    /// Unix seconds; set when hidden.
    #[serde(default)]
    pub hidden_at: u64,
    /// What to call it in Settings when the path says little (an i18n key for a
    /// group, an app name for leftovers).
    #[serde(default)]
    pub label: Option<String>,
    /// Written by test builds before kinds existed: `true` means a folder.
    #[serde(default, skip_serializing)]
    folder: bool,
}

impl Exclusion {
    #[cfg(test)]
    pub fn new(path: impl Into<String>, kind: HiddenKind, label: Option<String>) -> Self {
        Exclusion {
            path: path.into(),
            kind,
            hidden_at: 0,
            label,
            folder: false,
        }
    }

    fn is_path(&self) -> bool {
        matches!(self.kind, HiddenKind::File | HiddenKind::Folder)
    }
}

/// Names one hidden item, to show it again.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HiddenKey {
    pub path: String,
    pub kind: HiddenKind,
}

/// The exclusion list and the file it is saved in.
pub struct Exclusions {
    file: PathBuf,
    list: Mutex<Vec<Exclusion>>,
}

fn is_excluded(kept: &Kept, path: &Path) -> bool {
    kept.excluded.iter().any(|e| path.starts_with(e))
}

/// The file as it is now, if it is still the regular file the scan saw.
fn current(file: &ScannedFile) -> Option<Seen> {
    let meta = fs::symlink_metadata(&file.path).ok()?;
    let same = meta.is_file()
        && meta.len() == file.size
        && meta.modified().is_ok_and(|t| t == file.modified);
    same.then(|| Seen::from_meta(file.path.clone(), &meta))
}

/// Duplicate sets as they are now: only files still there and not excluded,
/// only sets that still have two copies or more. Oldest first, as scanned.
fn current_sets(kept: &Kept) -> Vec<Vec<Seen>> {
    kept.duplicate_sets
        .iter()
        .map(|set| {
            set.iter()
                .filter(|f| !is_excluded(kept, &f.path))
                .filter_map(current)
                .collect::<Vec<_>>()
        })
        .filter(|set| set.len() > 1)
        .collect()
}

/// The groups to show, rebuilt from the kept scan (see the module docs).
pub fn current_groups(kept: &Kept) -> Vec<FileGroup> {
    kept.groups
        .iter()
        .filter(|g| !kept.hidden_groups.contains(&g.id))
        .filter_map(|g| {
            if g.id == DUPLICATES {
                duplicates_group(kept, g)
            } else {
                files_group(kept, g)
            }
        })
        .collect()
}

fn files_group(kept: &Kept, g: &FileGroup) -> Option<FileGroup> {
    let excluded = |p: &str| is_excluded(kept, Path::new(p));
    let files = kept.files.get(&g.id).filter(|f| !f.is_empty());
    let Some(files) = files else {
        // Nothing to move as a group, only to look at (unknown folders): shown as
        // scanned, minus excluded items and folders moved one by one since the scan.
        let items: Vec<FileItem> = g
            .items
            .iter()
            .filter(|i| !excluded(&i.path) && fs::symlink_metadata(&i.path).is_ok())
            .cloned()
            .collect();
        let removed = (g.items.len() - items.len()) as u64;
        return (!items.is_empty()).then(|| FileGroup {
            size_bytes: items.iter().map(|i| i.size_bytes).sum(),
            item_count: g.item_count.saturating_sub(removed),
            items,
            ..g.clone()
        });
    };
    let present: Vec<&ScannedFile> = files
        .iter()
        .filter(|f| !is_excluded(kept, &f.path) && current(f).is_some())
        .collect();
    if present.is_empty() {
        return None;
    }
    let paths: HashSet<&Path> = present.iter().map(|f| f.path.as_path()).collect();
    let mut items: Vec<FileItem> = g
        .items
        .iter()
        .filter_map(|item| {
            let path = Path::new(&item.path);
            if paths.contains(path) {
                return Some(item.clone());
            }
            // A file that is gone, or an excluded folder.
            if item.last_used.is_some() || excluded(&item.path) {
                return None;
            }
            // A folder (like a temp folder): what is left in it.
            let size: u64 = present
                .iter()
                .filter(|f| f.path.starts_with(path))
                .map(|f| f.size)
                .sum();
            (size > 0).then(|| FileItem {
                size_bytes: size,
                ..item.clone()
            })
        })
        .collect();
    sort_items(&mut items);
    Some(FileGroup {
        size_bytes: present.iter().map(|f| f.size).sum(),
        item_count: present.len() as u64,
        items,
        ..g.clone()
    })
}

fn duplicates_group(kept: &Kept, g: &FileGroup) -> Option<FileGroup> {
    let mut items: Vec<FileItem> = current_sets(kept)
        .iter()
        .flat_map(|set| {
            let original = &set[0].path;
            set[1..].iter().map(move |copy| copy.item(Some(original)))
        })
        .collect();
    if items.is_empty() {
        return None;
    }
    let item_count = items.len() as u64;
    let size_bytes = items.iter().map(|i| i.size_bytes).sum();
    sort_items(&mut items);
    Some(FileGroup {
        size_bytes,
        item_count,
        items,
        ..g.clone()
    })
}

fn sort_items(items: &mut Vec<FileItem>) {
    items.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes).then(a.path.cmp(&b.path)));
    items.truncate(MAX_ITEMS);
}

/// What moving a whole group takes; see [`LastScan::group`].
pub fn movable(kept: &Kept, group_id: &str) -> Option<Vec<ScannedFile>> {
    if kept.hidden_groups.iter().any(|g| g == group_id) {
        return None;
    }
    let files = kept.files.get(group_id)?;
    if group_id == DUPLICATES {
        let copies = current_sets(kept)
            .iter()
            .flat_map(|set| set[1..].iter().map(Seen::scanned))
            .collect();
        return Some(copies);
    }
    Some(
        files
            .iter()
            .filter(|f| !is_excluded(kept, &f.path))
            .cloned()
            .collect(),
    )
}

/// The scanned files of a group among `paths`. Paths the scan did not find in
/// that group, and excluded ones, are ignored. For duplicates, any copy can be
/// picked, but one copy of every set always stays (the oldest of those picked).
pub fn pick(kept: &Kept, group_id: &str, paths: &[String]) -> Vec<ScannedFile> {
    if kept.hidden_groups.iter().any(|g| g == group_id) {
        return Vec::new();
    }
    let wanted: HashSet<&Path> = paths.iter().map(Path::new).collect();
    let chosen =
        |f: &&ScannedFile| wanted.contains(f.path.as_path()) && !is_excluded(kept, &f.path);
    if group_id == DUPLICATES {
        let mut out = Vec::new();
        for set in &kept.duplicate_sets {
            let present: Vec<&ScannedFile> = set.iter().filter(|f| current(f).is_some()).collect();
            let mut picked: Vec<&ScannedFile> = present.iter().copied().filter(chosen).collect();
            if !picked.is_empty() && picked.len() >= present.len() {
                picked.remove(0);
            }
            out.extend(picked.into_iter().cloned());
        }
        return out;
    }
    kept.files
        .get(group_id)
        .map(|files| files.iter().filter(chosen).cloned().collect())
        .unwrap_or_default()
}

fn review_file(seen: &Seen) -> ReviewFile {
    ReviewFile {
        path: seen.path.to_string_lossy().into_owned(),
        size_bytes: seen.size,
        last_used: iso_date(seen.last_used),
        modified: iso_date(seen.modified),
        can_open: can_open(&seen.path),
    }
}

/// Every file of a group, for the review. `None` for a group the scan did not find.
pub fn review(kept: &Kept, group_id: &str) -> Option<GroupReview> {
    let files = kept.files.get(group_id)?;
    if group_id == DUPLICATES {
        let mut sets = current_sets(kept);
        let wasted = |set: &Vec<Seen>| set[0].size * (set.len() as u64 - 1);
        sets.sort_by(|a, b| wasted(b).cmp(&wasted(a)).then(a[0].path.cmp(&b[0].path)));
        let total_count = sets.iter().map(|s| s.len() as u64 - 1).sum();
        let total_bytes = sets.iter().map(wasted).sum();
        sets.truncate(MAX_REVIEW_SETS);
        return Some(GroupReview {
            group_id: group_id.into(),
            total_count,
            total_bytes,
            files: Vec::new(),
            duplicate_sets: sets
                .iter()
                .map(|set| set.iter().map(review_file).collect())
                .collect(),
        });
    }
    let mut seen: Vec<Seen> = files
        .iter()
        .filter(|f| !is_excluded(kept, &f.path))
        .filter_map(current)
        .collect();
    seen.sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
    let total_count = seen.len() as u64;
    let total_bytes = seen.iter().map(|s| s.size).sum();
    seen.truncate(MAX_REVIEW_FILES);
    Some(GroupReview {
        group_id: group_id.into(),
        total_count,
        total_bytes,
        files: seen.iter().map(review_file).collect(),
        duplicate_sets: Vec::new(),
    })
}

/// True when the scan found this file (in any group or duplicate set).
fn is_known(kept: &Kept, path: &Path) -> bool {
    kept.files.values().flatten().any(|f| f.path == path)
        || kept.duplicate_sets.iter().flatten().any(|f| f.path == path)
}

fn can_open(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| OPENABLE.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Adds an exclusion (or replaces the one for the same item). Files and folders
/// need an absolute path; groups and leftovers a non-empty id.
fn add_to(list: &mut Vec<Exclusion>, mut exclusion: Exclusion, now: u64) -> Result<(), String> {
    if exclusion.path.trim().is_empty()
        || (exclusion.is_path() && !Path::new(&exclusion.path).is_absolute())
    {
        return Err("not a valid path or id".into());
    }
    list.retain(|e| !(e.path == exclusion.path && e.kind == exclusion.kind));
    if list.len() >= MAX_EXCLUSIONS {
        return Err("too many hidden items".into());
    }
    if exclusion.hidden_at == 0 {
        exclusion.hidden_at = now;
    }
    list.push(exclusion);
    Ok(())
}

impl Exclusions {
    fn load(file: PathBuf) -> Self {
        let mut list: Vec<Exclusion> = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        for e in list.iter_mut().filter(|e| e.folder) {
            e.kind = HiddenKind::Folder;
            e.folder = false;
        }
        Exclusions {
            file,
            list: Mutex::new(list),
        }
    }

    /// Changes the list, saves it, and applies it to the kept scan.
    fn change(
        &self,
        last: &LastScan,
        edit: impl FnOnce(&mut Vec<Exclusion>) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = list.clone();
        edit(&mut next)?;
        save(&self.file, &next).map_err(|e| e.to_string())?;
        apply(last, &next);
        *list = next;
        Ok(())
    }

    fn list(&self) -> Vec<Exclusion> {
        self.list.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

fn apply(last: &LastScan, list: &[Exclusion]) {
    let mut kept = last.0.lock().unwrap_or_else(|e| e.into_inner());
    kept.excluded = list
        .iter()
        .filter(|e| e.is_path())
        .map(|e| PathBuf::from(&e.path))
        .collect();
    kept.hidden_groups = list
        .iter()
        .filter(|e| e.kind == HiddenKind::Group)
        .map(|e| e.path.clone())
        .collect();
}

/// The leftovers the user hid, by path (or registry key), for the leftovers to leave out.
pub fn hidden_leftovers(app: &AppHandle) -> Vec<String> {
    app.state::<Exclusions>()
        .list()
        .into_iter()
        .filter(|e| e.kind == HiddenKind::Leftover)
        .map(|e| e.path)
        .collect()
}

/// Whether a leftover at `path` was hidden. Windows paths and registry keys
/// ignore case, so the comparison does too.
pub fn hides_leftover(hidden: &[String], path: &str) -> bool {
    hidden.iter().any(|h| h.eq_ignore_ascii_case(path))
}

/// Writes to a temporary file first, so a crash never leaves half a list.
fn save(file: &Path, list: &[Exclusion]) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_vec_pretty(list).map_err(io::Error::other)?;
    let tmp = file.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, file)
}

/// Loads the saved exclusions and applies them to the (empty) kept scan.
pub fn init(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let file = app.path().app_local_data_dir()?.join(EXCLUSIONS_FILE);
    let exclusions = Exclusions::load(file);
    apply(&app.state::<LastScan>(), &exclusions.list());
    app.manage(exclusions);
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Runs `f` with the kept scan on a background thread: checking files on disk takes a moment.
async fn with_kept<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Kept) -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let last = app.state::<LastScan>();
        let kept = last.0.lock().map_err(|e| e.to_string())?;
        Ok(f(&kept))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The groups of the last scan as they are now (after a move, an undo or an exclusion).
#[tauri::command]
pub async fn current_file_groups(app: AppHandle) -> Result<Vec<FileGroup>, String> {
    with_kept(app, current_groups).await
}

#[tauri::command]
pub async fn review_file_group(
    app: AppHandle,
    group_id: String,
) -> Result<Option<GroupReview>, String> {
    with_kept(app, move |kept| review(kept, &group_id)).await
}

/// Removes the picked files through [`cleanup::remove`], like whole groups: one
/// quarantine entry per group, or deleted right away for temporary files and caches.
#[tauri::command]
pub async fn quarantine_file_selection(
    app: AppHandle,
    selections: Vec<FileSelection>,
) -> Result<MoveReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked: Vec<(String, Vec<ScannedFile>)> = {
            let last = app.state::<LastScan>();
            let kept = last.0.lock().map_err(|e| e.to_string())?;
            selections
                .iter()
                .filter(|s| kept.groups.iter().any(|g| g.id == s.group_id))
                .map(|s| (s.group_id.clone(), pick(&kept, &s.group_id, &s.paths)))
                .collect()
        };
        let quarantine = app.state::<Quarantine>();
        let mut report = MoveReport::default();
        let mut names = Vec::new();
        let mut result = Ok(());
        for (group_id, files) in picked.into_iter().filter(|(_, f)| !f.is_empty()) {
            names.push(cleanup::group_title(&group_id));
            result = cleanup::remove(&quarantine, &group_id, &files, now_secs(), &mut report);
            if result.is_err() {
                break;
            }
        }
        history::record_move(&app, history::Kind::Files, names, &report);
        result.map_err(|e| e.to_string())?;
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn list_exclusions(exclusions: tauri::State<'_, Exclusions>) -> Vec<Exclusion> {
    exclusions.list()
}

/// Hides items (all or none: one invalid item changes nothing).
#[tauri::command]
pub async fn add_exclusions(app: AppHandle, items: Vec<Exclusion>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let last = app.state::<LastScan>();
        let now = now_secs();
        app.state::<Exclusions>().change(&last, |list| {
            items.into_iter().try_for_each(|e| add_to(list, e, now))
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Shows items again. Nothing moves: they are simply listed again.
#[tauri::command]
pub async fn remove_exclusions(app: AppHandle, items: Vec<HiddenKey>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let last = app.state::<LastScan>();
        app.state::<Exclusions>().change(&last, |list| {
            list.retain(|e| !items.iter().any(|k| k.path == e.path && k.kind == e.kind));
            Ok(())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A small preview made by Windows (the same one Explorer shows), as a PNG
/// `data:` URL. `None` when Windows has no preview for the file (it is not a
/// photo, video or document) or the scan did not find it.
#[tauri::command]
pub async fn file_thumbnail(
    app: AppHandle,
    path: String,
    size: u32,
) -> Result<Option<String>, String> {
    let path = PathBuf::from(path);
    let known = {
        let check = path.clone();
        with_kept(app, move |kept| is_known(kept, &check)).await?
    };
    if !known {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(move || platform::thumbnail(&path, size.clamp(32, 256)))
        .await
        .map_err(|e| e.to_string())
}

/// Opens a photo, video or document found by the scan in its usual app.
#[tauri::command]
pub async fn open_scanned_file(app: AppHandle, path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    let check = path.clone();
    if !with_kept(app, move |kept| is_known(kept, &check)).await? || !can_open(&path) {
        return Err("this file cannot be opened from here".into());
    }
    platform::open(&path).map_err(|e| e.to_string())
}

/// Opens the file's folder in Explorer with the file selected.
#[tauri::command]
pub async fn show_scanned_file(app: AppHandle, path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    let check = path.clone();
    if !with_kept(app, move |kept| is_known(kept, &check)).await? {
        return Err("this file was not found by the scan".into());
    }
    platform::reveal(&path).map_err(|e| e.to_string())
}

#[cfg(windows)]
pub(crate) mod platform {
    use std::io;
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;
    use windows::core::{w, HSTRING};
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::DeleteObject;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, ShellExecuteW, SIIGBF_THUMBNAILONLY,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    pub fn thumbnail(path: &Path, size: u32) -> Option<String> {
        // SAFETY: COM is set up for this thread and undone below when we did set it up.
        let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let result = (|| {
            // SAFETY: plain COM calls; the bitmap is ours and deleted below.
            unsafe {
                let factory: IShellItemImageFactory =
                    SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok()?;
                let side = size as i32;
                let bitmap = factory
                    .GetImage(SIZE { cx: side, cy: side }, SIIGBF_THUMBNAILONLY)
                    .ok()?;
                let url = crate::apps::icon::from_bitmap(bitmap);
                let _ = DeleteObject(bitmap.into());
                url
            }
        })();
        if init.is_ok() {
            // SAFETY: matches the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
        }
        result
    }

    pub fn open(path: &Path) -> io::Result<()> {
        // SAFETY: plain call with valid strings.
        let result = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                &HSTRING::from(path.as_os_str()),
                None,
                None,
                SW_SHOWNORMAL,
            )
        };
        // Values above 32 mean success.
        if result.0 as usize > 32 {
            Ok(())
        } else {
            Err(io::Error::other("Windows could not open the file"))
        }
    }

    pub fn reveal(path: &Path) -> io::Result<()> {
        // Explorer wants `/select,"C:\path"` as one argument, quoted its own way.
        Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map(|_| ())
    }
}

/// Easy Uninstaller targets Windows; elsewhere there are no previews and nothing opens.
#[cfg(not(windows))]
pub(crate) mod platform {
    use std::io;
    use std::path::Path;

    pub fn thumbnail(_path: &Path, _size: u32) -> Option<String> {
        None
    }

    pub fn open(_path: &Path) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    pub fn reveal(_path: &Path) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Safety;
    use std::collections::HashMap;

    const NOW: u64 = 1_800_000_000;

    /// A fresh folder of files, removed when dropped.
    struct Sandbox(PathBuf);

    impl Sandbox {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "easy-uninstaller-review-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Sandbox(dir)
        }

        fn file(&self, rel: &str, content: &str) -> ScannedFile {
            let path = self.0.join(rel);
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

    fn group(id: &str, items: Vec<FileItem>) -> FileGroup {
        FileGroup {
            id: id.into(),
            title_key: format!("files.groups.{id}.title"),
            description_key: format!("files.groups.{id}.description"),
            safety: Safety::Check,
            size_bytes: items.iter().map(|i| i.size_bytes).sum(),
            item_count: items.len() as u64,
            items,
            review_only: false,
        }
    }

    fn item(f: &ScannedFile, last_used: Option<&str>) -> FileItem {
        FileItem {
            path: f.path.to_string_lossy().into_owned(),
            size_bytes: f.size,
            last_used: last_used.map(str::to_string),
            duplicate_of: None,
            explain_id: None,
            original_path: None,
        }
    }

    fn paths(files: &[ScannedFile]) -> Vec<String> {
        files
            .iter()
            .map(|f| f.path.to_string_lossy().into_owned())
            .collect()
    }

    /// A kept scan with a temp folder, two large files and a set of three duplicates.
    struct Fixture {
        s: Sandbox,
        kept: Kept,
        tmp: Vec<ScannedFile>,
        large: Vec<ScannedFile>,
        dups: Vec<ScannedFile>,
    }

    fn fixture(name: &str) -> Fixture {
        let s = Sandbox::new(name);
        let tmp = vec![s.file("Temp/a.tmp", "aaaa"), s.file("Temp/sub/b.tmp", "bb")];
        let large = vec![
            s.file("Videos/movie.mkv", "123456"),
            s.file("Work/backup.zip", "12345"),
        ];
        let dups = vec![
            s.file("Pictures/photo.jpg", "same"),
            s.file("Downloads/photo.jpg", "same"),
            s.file("Desktop/photo (1).jpg", "same"),
        ];
        let temp_dir = ScannedFile {
            path: s.0.join("Temp"),
            size: 6,
            modified: UNIX_EPOCH,
        };
        let mut files = HashMap::new();
        files.insert("temp".to_string(), tmp.clone());
        files.insert("large-files".to_string(), large.clone());
        files.insert(DUPLICATES.to_string(), dups[1..].to_vec());
        let kept = Kept {
            groups: vec![
                group("temp", vec![item(&temp_dir, None)]),
                group(DUPLICATES, vec![]),
                group(
                    "large-files",
                    large.iter().map(|f| item(f, Some("2020-01-01"))).collect(),
                ),
            ],
            files,
            duplicate_sets: vec![dups.clone()],
            excluded: Vec::new(),
            hidden_groups: Vec::new(),
        };
        Fixture {
            s,
            kept,
            tmp,
            large,
            dups,
        }
    }

    fn find<'a>(groups: &'a [FileGroup], id: &str) -> &'a FileGroup {
        groups.iter().find(|g| g.id == id).unwrap()
    }

    #[test]
    fn rebuilds_groups_from_what_is_still_there() {
        let mut f = fixture("rebuild");
        let groups = current_groups(&f.kept);
        assert_eq!(groups.len(), 3);
        assert_eq!(find(&groups, "temp").size_bytes, 6);
        let dups = find(&groups, DUPLICATES);
        assert_eq!((dups.item_count, dups.size_bytes), (2, 8));
        assert!(dups
            .items
            .iter()
            .all(|i| i.duplicate_of.as_deref() == Some(f.dups[0].path.to_str().unwrap())));

        // A temp file and a large file are gone; the original photo was moved.
        fs::remove_file(&f.tmp[1].path).unwrap();
        fs::remove_file(&f.large[0].path).unwrap();
        fs::remove_file(&f.dups[0].path).unwrap();
        // The user hides the backup's folder.
        f.kept.excluded.push(f.s.0.join("Work"));
        let groups = current_groups(&f.kept);
        let temp = find(&groups, "temp");
        assert_eq!((temp.item_count, temp.size_bytes), (1, 4));
        assert_eq!(temp.items[0].size_bytes, 4, "folder size follows its files");
        assert!(
            groups.iter().all(|g| g.id != "large-files"),
            "nothing left to show"
        );
        let dups = find(&groups, DUPLICATES);
        assert_eq!(
            dups.item_count, 1,
            "the oldest copy left is now the one kept"
        );
        assert_eq!(
            dups.items[0].duplicate_of.as_deref(),
            Some(f.dups[1].path.to_str().unwrap())
        );
    }

    #[test]
    fn picks_only_scanned_files_and_always_keeps_one_copy() {
        let mut f = fixture("pick");
        let stranger = f.s.file("Documents/taxes.pdf", "secret");
        let mut wanted = paths(&f.large);
        wanted.push(stranger.path.to_string_lossy().into_owned());
        let picked = pick(&f.kept, "large-files", &wanted);
        assert_eq!(picked, f.large, "a path the scan did not find is ignored");
        assert!(
            pick(&f.kept, "temp", &paths(&f.large)).is_empty(),
            "wrong group"
        );

        // Keep the newest copy instead: the original may go.
        let picked = pick(&f.kept, DUPLICATES, &paths(&f.dups[..2]));
        assert_eq!(picked, f.dups[..2].to_vec());
        // Asking for every copy still leaves the oldest one.
        let picked = pick(&f.kept, DUPLICATES, &paths(&f.dups));
        assert_eq!(picked, f.dups[1..].to_vec());

        f.kept.excluded.push(f.large[1].path.clone());
        assert_eq!(
            pick(&f.kept, "large-files", &paths(&f.large)),
            f.large[..1].to_vec()
        );
        assert_eq!(
            movable(&f.kept, "large-files").unwrap(),
            f.large[..1].to_vec()
        );
    }

    #[test]
    fn moving_a_whole_duplicates_group_keeps_a_copy_of_each_set() {
        let f = fixture("movable");
        assert_eq!(movable(&f.kept, DUPLICATES).unwrap(), f.dups[1..].to_vec());
        // The kept copy was moved by hand earlier: the next one stays.
        fs::remove_file(&f.dups[0].path).unwrap();
        assert_eq!(movable(&f.kept, DUPLICATES).unwrap(), f.dups[2..].to_vec());
        assert!(movable(&f.kept, "unknown").is_none());
    }

    #[test]
    fn reviews_files_and_duplicate_sets() {
        let f = fixture("review");
        let r = review(&f.kept, "large-files").unwrap();
        assert_eq!((r.total_count, r.total_bytes), (2, 11));
        assert!(r.files[0].path.ends_with("movie.mkv"), "largest first");
        assert!(r.files[0].can_open && r.files[1].can_open);
        assert_eq!(r.files[0].last_used.len(), 10);

        let r = review(&f.kept, DUPLICATES).unwrap();
        assert_eq!((r.total_count, r.total_bytes), (2, 8));
        assert!(r.files.is_empty());
        assert_eq!(r.duplicate_sets.len(), 1);
        assert_eq!(
            r.duplicate_sets[0][0].path,
            f.dups[0].path.to_str().unwrap()
        );
        assert!(review(&f.kept, "unknown").is_none());
    }

    #[test]
    fn knows_scanned_files_and_what_can_be_opened() {
        let f = fixture("known");
        assert!(is_known(&f.kept, &f.dups[0].path), "originals count too");
        assert!(is_known(&f.kept, &f.tmp[0].path));
        assert!(!is_known(&f.kept, &f.s.0.join("Temp")));
        assert!(can_open(Path::new(r"C:\x\Photo.JPG")));
        assert!(!can_open(Path::new(r"C:\x\setup.exe")));
        assert!(!can_open(Path::new(r"C:\x\run.bat")));
        assert!(!can_open(Path::new(r"C:\x\noext")));
    }

    #[test]
    fn hides_leftovers_by_path_ignoring_case() {
        let hidden = vec![
            r"C:\Users\me\AppData\Roaming\Zoom".to_string(),
            r"HKCU\Software\Zoom".to_string(),
        ];
        assert!(hides_leftover(&hidden, r"c:\users\me\appdata\roaming\zoom"));
        assert!(hides_leftover(&hidden, r"HKCU\SOFTWARE\Zoom"));
        assert!(!hides_leftover(
            &hidden,
            r"C:\Users\me\AppData\Roaming\Zoom\Logs"
        ));
        assert!(!hides_leftover(&[], r"HKCU\Software\Zoom"));
    }

    #[test]
    fn hidden_groups_are_not_shown_or_moved() {
        let mut f = fixture("hidden-group");
        f.kept.hidden_groups.push("large-files".into());
        assert!(current_groups(&f.kept)
            .iter()
            .all(|g| g.id != "large-files"));
        assert!(movable(&f.kept, "large-files").is_none());
        assert!(pick(&f.kept, "large-files", &paths(&f.large)).is_empty());
        assert_eq!(current_groups(&f.kept).len(), 2);
    }

    #[test]
    fn saves_and_validates_exclusions() {
        let s = Sandbox::new("exclusions");
        let file = s.0.join("data").join(EXCLUSIONS_FILE);
        let last = LastScan::default();
        let ex = Exclusions::load(file.clone());
        let abs = s.0.join("Work").to_string_lossy().into_owned();
        let items = vec![
            Exclusion::new(abs.clone(), HiddenKind::Folder, None),
            Exclusion::new(
                "large-files",
                HiddenKind::Group,
                Some("files.groups.largeFiles.title".into()),
            ),
            Exclusion::new("zoom-1", HiddenKind::Leftover, Some("Zoom".into())),
        ];
        ex.change(&last, |l| {
            items.into_iter().try_for_each(|e| add_to(l, e, NOW))
        })
        .unwrap();
        let bad = Exclusion::new("relative\\x", HiddenKind::File, None);
        assert!(ex.change(&last, |l| add_to(l, bad, NOW)).is_err());
        let bad = Exclusion::new(" ", HiddenKind::Group, None);
        assert!(ex.change(&last, |l| add_to(l, bad, NOW)).is_err());
        {
            let kept = last.0.lock().unwrap();
            assert_eq!(kept.excluded, [PathBuf::from(&abs)]);
            assert_eq!(kept.hidden_groups, ["large-files"]);
        }
        assert!(ex.list().iter().all(|e| e.hidden_at == NOW));

        let again = Exclusions::load(file.clone());
        assert_eq!(again.list(), ex.list(), "saved to disk");
        again
            .change(&last, |l| {
                l.retain(|e| e.kind != HiddenKind::Folder);
                Ok(())
            })
            .unwrap();
        assert!(last.0.lock().unwrap().excluded.is_empty());

        // A list written by an older test build.
        fs::write(&file, format!(r#"[{{"path":{abs:?},"folder":true}}]"#)).unwrap();
        let old = Exclusions::load(file);
        assert_eq!(old.list()[0].kind, HiddenKind::Folder);
    }
}
