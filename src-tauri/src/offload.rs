//! Freeing space on the system drive without deleting anything.
//!
//! 1. **OneDrive "online only".** Files already saved on OneDrive can keep only
//!    their cloud copy: they stay in their folders and download again when opened.
//!    The app sets the same "unpinned" attribute as OneDrive's own "Free up space"
//!    (Files On-Demand); OneDrive then removes the local copy itself, and only of
//!    files it has uploaded. Nothing is deleted, on the PC or on OneDrive.
//! 2. **Move to another drive.** Big videos, archives, disk images and lossless
//!    audio in the user's folders move to a folder on another fixed drive. Every
//!    move is recorded so it can be moved back from the app. Files inside OneDrive
//!    are never moved (that would take them out of OneDrive), nor are apps or games,
//!    which would stop working.
//!
//! Like the file scan, the scan is read only and keeps the full lists here: the UI
//! only says which of the listed files to leave out.

use crate::disk::{self, DriveInfo};
use crate::files::{self, Seen};
use crate::quarantine::{move_file, now_secs};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;
/// Where the moves are recorded, in the app's data folder.
const RECORDS: &str = "moved-files.json";
/// Space always left free on the drive files are moved to.
const KEEP_FREE: u64 = 2 * GB;
/// Kinds of files that open from anywhere: moving them breaks nothing.
const MOVE_EXTENSIONS: [&str; 22] = [
    // Videos
    "mp4", "mkv", "mov", "avi", "wmv", "m4v", "webm", "mts", "m2ts", "mpg", "mpeg", "vob",
    // Archives and backups
    "zip", "7z", "rar", "tar", "gz", "tgz", // Disk images
    "iso", "img", // Lossless audio
    "wav", "flac",
];

/// Which files are offered.
#[derive(Debug, Clone)]
struct Rules {
    /// Small files give little space back and would need the internet to open.
    online_min_bytes: u64,
    /// Files changed recently are probably being worked on: they stay on the PC.
    online_min_age: Duration,
    move_min_bytes: u64,
    /// Still being downloaded or written, perhaps.
    move_min_age: Duration,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            online_min_bytes: MB,
            online_min_age: 30 * DAY,
            move_min_bytes: 100 * MB,
            move_min_age: DAY,
        }
    }
}

/// A file offered by one of the two cards.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OffloadFile {
    pub path: String,
    pub size_bytes: u64,
    /// "YYYY-MM-DD": the later of last opened and last changed.
    pub last_used: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OneDriveInfo {
    /// OneDrive folders on this PC; empty when OneDrive is not set up.
    pub folders: Vec<String>,
    /// Files on the PC and on OneDrive that can be made online only.
    pub file_count: u64,
    pub size_bytes: u64,
    /// Largest first, at most 200.
    pub files: Vec<OffloadFile>,
    /// Marked online only, but OneDrive has not removed the copy on the PC yet.
    pub freeing_count: u64,
    pub freeing_bytes: u64,
}

/// A file moved to another drive, which can be moved back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MovedFile {
    pub id: String,
    /// Where it was.
    pub original_path: String,
    /// Where it is now.
    pub path: String,
    pub size_bytes: u64,
    /// Unix seconds.
    pub moved_at: u64,
    /// The folder the app made on the other drive (removed once empty again).
    #[serde(default)]
    pub base: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MoveInfo {
    /// The drive files are moved from, `"C:"`.
    pub system_drive: String,
    /// The drives files can go to, most free space first; empty when there is none.
    pub drives: Vec<DriveInfo>,
    pub file_count: u64,
    pub size_bytes: u64,
    /// Largest first, at most 200.
    pub files: Vec<OffloadFile>,
    /// Moved before and still there, newest first.
    pub moved: Vec<MovedFile>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OffloadScan {
    pub one_drive: OneDriveInfo,
    pub move_files: MoveInfo,
}

/// What "Free up space" or its undo did.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OnlineReport {
    pub done_count: u64,
    pub done_bytes: u64,
    /// Changed since the scan, gone, or refused by Windows.
    pub skipped_count: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MoveReport {
    /// The moves, to undo them.
    pub ids: Vec<String>,
    pub moved_count: u64,
    pub moved_bytes: u64,
    /// In use, changed since the scan, or gone.
    pub skipped_count: u64,
    /// Left in place because the other drive is nearly full.
    pub no_room_count: u64,
    /// The folder on the other drive.
    pub folder: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MoveBackReport {
    pub restored_count: u64,
    pub restored_bytes: u64,
    /// Came back as "name (2)" because the original name is taken.
    pub renamed_count: u64,
    /// Not found on the other drive anymore, or in use.
    pub failed_count: u64,
}

/// Sent after each file while files are moved.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveProgress {
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_count: u64,
    pub total_count: u64,
}

// ── OneDrive attributes ─────────────────────────────────────────────────────

const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
const FILE_ATTRIBUTE_ARCHIVE: u32 = 0x20;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
const FILE_ATTRIBUTE_NOT_CONTENT_INDEXED: u32 = 0x2000;
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x40000;
const FILE_ATTRIBUTE_PINNED: u32 = 0x80000;
const FILE_ATTRIBUTE_UNPINNED: u32 = 0x100000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x400000;
/// The attributes `SetFileAttributesW` takes; the others are kept by Windows.
const SETTABLE: u32 = FILE_ATTRIBUTE_READONLY
    | FILE_ATTRIBUTE_HIDDEN
    | FILE_ATTRIBUTE_SYSTEM
    | FILE_ATTRIBUTE_ARCHIVE
    | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED
    | FILE_ATTRIBUTE_PINNED
    | FILE_ATTRIBUTE_UNPINNED;

/// Where a file in a OneDrive folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cloud {
    /// Not on OneDrive yet (not uploaded, or a kind OneDrive does not sync).
    NotSynced,
    /// Only online already: it uses no space on the PC.
    OnlineOnly,
    /// Marked online only; OneDrive has not removed the copy on the PC yet.
    Freeing,
    /// On the PC and on OneDrive.
    Both,
}

/// OneDrive turns every file it has uploaded into a "placeholder" (a reparse
/// point); files only online carry a "recall" attribute.
fn cloud_state(attrs: u32) -> Cloud {
    let not_local = FILE_ATTRIBUTE_OFFLINE
        | FILE_ATTRIBUTE_RECALL_ON_OPEN
        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
    if attrs & not_local != 0 {
        Cloud::OnlineOnly
    } else if attrs & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        Cloud::NotSynced
    } else if attrs & FILE_ATTRIBUTE_UNPINNED != 0 {
        Cloud::Freeing
    } else {
        Cloud::Both
    }
}

/// "Free up space": unpinned, and no longer "always keep on this device".
fn online_only_attrs(attrs: u32) -> u32 {
    (attrs & SETTABLE & !FILE_ATTRIBUTE_PINNED) | FILE_ATTRIBUTE_UNPINNED
}

/// "Always keep on this device": OneDrive downloads the file again.
fn keep_attrs(attrs: u32) -> u32 {
    (attrs & SETTABLE & !FILE_ATTRIBUTE_UNPINNED) | FILE_ATTRIBUTE_PINNED
}

// ── Scanning ────────────────────────────────────────────────────────────────

/// Where to look. Filled per platform by [`platform::places`].
#[derive(Debug, Default)]
struct Places {
    onedrive: Vec<PathBuf>,
    /// The user's folder, searched for files to move.
    profile: Option<PathBuf>,
}

#[derive(Debug, Default)]
struct Found {
    onedrive: Vec<PathBuf>,
    online: Vec<Seen>,
    freeing_count: u64,
    freeing_bytes: u64,
    movable: Vec<Seen>,
}

fn lower(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

/// `path` is `root` or inside it, ignoring case as Windows does.
fn is_under(path: &Path, root: &Path) -> bool {
    let (path, root) = (lower(path), lower(root));
    let root = root.trim_end_matches(['\\', '/']);
    path.strip_prefix(root)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['\\', '/']))
}

fn is_movable_kind(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MOVE_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Read only. `attributes` gives a file's Windows attributes.
fn find(
    places: &Places,
    now: SystemTime,
    rules: &Rules,
    attributes: &dyn Fn(&Path, &Metadata) -> u32,
) -> Found {
    let old = |t: SystemTime, age: Duration| now.duration_since(t).is_ok_and(|d| d >= age);
    // The same folder can come from the environment and the registry, in another case.
    let mut roots = places.onedrive.clone();
    roots.sort_by_key(|p| lower(p));
    roots.dedup_by_key(|p| lower(p));
    let onedrive: Vec<PathBuf> = files::dedup_dirs(&roots)
        .into_iter()
        .filter(|p| p.is_dir())
        .collect();
    let mut found = Found::default();
    for root in &onedrive {
        files::walk(root, &|_| false, &mut |path, meta| {
            let state = cloud_state(attributes(&path, meta));
            let seen = Seen::from_meta(path, meta);
            match state {
                Cloud::Both
                    if seen.size >= rules.online_min_bytes
                        && old(seen.modified, rules.online_min_age) =>
                {
                    found.online.push(seen)
                }
                Cloud::Freeing => {
                    found.freeing_count += 1;
                    found.freeing_bytes += seen.size;
                }
                _ => {}
            }
        });
    }
    if let Some(profile) = &places.profile {
        let in_onedrive = |p: &Path| onedrive.iter().any(|o| is_under(p, o));
        let skip = |p: &Path| files::skip_personal_dir(p) || in_onedrive(p);
        files::walk(profile, &skip, &mut |path, meta| {
            if meta.len() >= rules.move_min_bytes
                && is_movable_kind(&path)
                && !in_onedrive(&path)
                && meta.modified().is_ok_and(|t| old(t, rules.move_min_age))
            {
                found.movable.push(Seen::from_meta(path, meta));
            }
        });
    }
    let by_size = |a: &Seen, b: &Seen| b.size.cmp(&a.size).then(a.path.cmp(&b.path));
    found.online.sort_by(by_size);
    found.movable.sort_by(by_size);
    found.onedrive = onedrive;
    found
}

fn listed(files: &[Seen]) -> Vec<OffloadFile> {
    files
        .iter()
        .take(files::MAX_ITEMS)
        .map(|s| OffloadFile {
            path: s.path.to_string_lossy().into_owned(),
            size_bytes: s.size,
            last_used: files::iso_date(s.last_used),
        })
        .collect()
}

/// The scanned file, still as the scan saw it (same size, same last change).
fn unchanged(seen: &Seen, meta: &Metadata) -> bool {
    meta.is_file() && meta.len() == seen.size && meta.modified().is_ok_and(|t| t == seen.modified)
}

// ── Kept state ──────────────────────────────────────────────────────────────

/// The last scan and what was done since, so the cards stay right without
/// scanning again.
#[derive(Default)]
pub struct LastOffload(Mutex<Kept>);

#[derive(Debug, Default)]
struct Kept {
    onedrive: Vec<PathBuf>,
    online: Vec<Seen>,
    freeing_count: u64,
    freeing_bytes: u64,
    /// The files of the last "Free up space", for its undo.
    freed: Vec<Seen>,
    movable: Vec<Seen>,
}

impl LastOffload {
    fn lock(&self) -> MutexGuard<'_, Kept> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Kept {
    fn snapshot(&self, drives: Vec<DriveInfo>, moved: Vec<MovedFile>) -> OffloadScan {
        OffloadScan {
            one_drive: OneDriveInfo {
                folders: self
                    .onedrive
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect(),
                file_count: self.online.len() as u64,
                size_bytes: self.online.iter().map(|s| s.size).sum(),
                files: listed(&self.online),
                freeing_count: self.freeing_count,
                freeing_bytes: self.freeing_bytes,
            },
            move_files: MoveInfo {
                system_drive: disk::system_drive(),
                drives,
                file_count: self.movable.len() as u64,
                size_bytes: self.movable.iter().map(|s| s.size).sum(),
                files: listed(&self.movable),
                moved,
            },
        }
    }

    /// Takes the files of `list` not named in `except` out of it.
    fn take(list: &mut Vec<Seen>, except: &[String]) -> Vec<Seen> {
        let except: HashSet<&str> = except.iter().map(String::as_str).collect();
        let (taken, left) = std::mem::take(list)
            .into_iter()
            .partition(|s| !except.contains(s.path.to_string_lossy().as_ref()));
        *list = left;
        taken
    }
}

/// Sets "online only" on the files still on the PC and on OneDrive. Returns the
/// report and the files done.
fn free_up(
    files: Vec<Seen>,
    attributes: &dyn Fn(&Path, &Metadata) -> u32,
    set: &dyn Fn(&Path, u32) -> io::Result<()>,
) -> (OnlineReport, Vec<Seen>, Vec<Seen>) {
    let mut report = OnlineReport::default();
    let (mut done, mut left) = (Vec::new(), Vec::new());
    for seen in files {
        let ok = fs::symlink_metadata(&seen.path).is_ok_and(|meta| {
            let attrs = attributes(&seen.path, &meta);
            unchanged(&seen, &meta)
                && cloud_state(attrs) == Cloud::Both
                && set(&seen.path, online_only_attrs(attrs)).is_ok()
        });
        if ok {
            report.done_count += 1;
            report.done_bytes += seen.size;
            done.push(seen);
        } else {
            report.skipped_count += 1;
            left.push(seen);
        }
    }
    (report, done, left)
}

/// Undoes [`free_up`]: the files are kept on this device again.
fn keep_again(
    files: &[Seen],
    attributes: &dyn Fn(&Path, &Metadata) -> u32,
    set: &dyn Fn(&Path, u32) -> io::Result<()>,
) -> OnlineReport {
    let mut report = OnlineReport::default();
    for seen in files {
        let ok = fs::symlink_metadata(&seen.path).is_ok_and(|meta| {
            meta.is_file() && set(&seen.path, keep_attrs(attributes(&seen.path, &meta))).is_ok()
        });
        if ok {
            report.done_count += 1;
            report.done_bytes += seen.size;
        } else {
            report.skipped_count += 1;
        }
    }
    report
}

// ── Moving to another drive ─────────────────────────────────────────────────

/// The folder name comes from the UI (it is translated): one plain name only.
fn valid_folder_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.len() <= 80
        && name != "."
        && name != ".."
        && !name.ends_with('.')
        && !name
            .chars()
            .any(|c| c.is_control() || r#"<>:"/\|?*"#.contains(c))
}

/// `"D:"` -> `D:\`, only for a plain drive letter.
fn drive_root(drive: &str) -> Option<PathBuf> {
    let b = drive.as_bytes();
    (b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
        .then(|| PathBuf::from(format!("{}\\", drive.to_ascii_uppercase())))
}

/// Where a file goes: the same place under `base` as under the user's folder
/// (`Videos\Holiday.mp4`), or just its name for files elsewhere.
fn target_for(base: &Path, profile: Option<&Path>, path: &Path) -> PathBuf {
    let rel = profile
        .and_then(|p| path.strip_prefix(p).ok())
        .filter(|r| {
            r.components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        })
        .map(Path::to_path_buf)
        .or_else(|| path.file_name().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("file"));
    base.join(rel)
}

/// `path` if it is free, otherwise `name (2).ext`, `name (3).ext`, ...
fn free_name(path: &Path) -> PathBuf {
    if fs::symlink_metadata(path).is_err() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map_or("".into(), |s| s.to_string_lossy());
    let ext = path
        .extension()
        .map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|p| fs::symlink_metadata(p).is_err())
        .expect("some name is free")
}

fn new_id(now: u64, records: &[MovedFile]) -> String {
    (0..)
        .map(|n| format!("{now}-{n}"))
        .find(|id| !records.iter().any(|r| &r.id == id))
        .expect("some id is free")
}

/// Moves `files` under `base`, recording each move in `records` (saved after
/// every file by `save`, so a crash never loses track of a moved file).
/// `room` is the free space on the target drive.
#[allow(clippy::too_many_arguments)]
fn move_files(
    files: &[Seen],
    profile: Option<&Path>,
    base: &Path,
    mut room: u64,
    now: u64,
    records: &mut Vec<MovedFile>,
    save: &mut dyn FnMut(&[MovedFile]),
    progress: &mut dyn FnMut(MoveProgress),
) -> MoveReport {
    let mut report = MoveReport {
        folder: base.to_string_lossy().into_owned(),
        ..MoveReport::default()
    };
    let total_bytes = files.iter().map(|s| s.size).sum();
    let mut done_bytes = 0;
    for (i, seen) in files.iter().enumerate() {
        if seen.size.saturating_add(KEEP_FREE) > room {
            report.no_room_count += 1;
        } else if fs::symlink_metadata(&seen.path).is_ok_and(|m| unchanged(seen, &m)) {
            let target = free_name(&target_for(base, profile, &seen.path));
            let moved = target
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| move_file(&seen.path, &target));
            match moved {
                Ok(_) => {
                    let id = new_id(now, records);
                    records.push(MovedFile {
                        id: id.clone(),
                        original_path: seen.path.to_string_lossy().into_owned(),
                        path: target.to_string_lossy().into_owned(),
                        size_bytes: seen.size,
                        moved_at: now,
                        base: base.to_string_lossy().into_owned(),
                    });
                    save(records);
                    report.ids.push(id);
                    report.moved_count += 1;
                    report.moved_bytes += seen.size;
                    room -= seen.size;
                }
                Err(_) => {
                    remove_empty_dirs(&target, base);
                    report.skipped_count += 1;
                }
            }
        } else {
            report.skipped_count += 1;
        }
        done_bytes += seen.size;
        progress(MoveProgress {
            done_bytes,
            total_bytes,
            done_count: i as u64 + 1,
            total_count: files.len() as u64,
        });
    }
    report
}

/// Moves the recorded files back where they were. Returns the files back home,
/// as the scan would see them.
fn move_back(
    ids: &[String],
    records: &mut Vec<MovedFile>,
    report: &mut MoveBackReport,
) -> Vec<Seen> {
    let mut back = Vec::new();
    for id in ids {
        let Some(pos) = records.iter().position(|r| &r.id == id) else {
            report.failed_count += 1;
            continue;
        };
        let record = records[pos].clone();
        let stored = PathBuf::from(&record.path);
        let original = PathBuf::from(&record.original_path);
        if !fs::symlink_metadata(&stored).is_ok_and(|m| m.is_file()) {
            report.failed_count += 1;
            continue;
        }
        let target = free_name(&original);
        let moved = original
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| move_file(&stored, &target));
        if moved.is_err() {
            report.failed_count += 1;
            continue;
        }
        records.remove(pos);
        report.restored_count += 1;
        report.restored_bytes += record.size_bytes;
        if target != original {
            report.renamed_count += 1;
        }
        if !record.base.is_empty() {
            remove_empty_dirs(&stored, Path::new(&record.base));
        }
        if let Ok(meta) = fs::symlink_metadata(&target) {
            back.push(Seen::from_meta(target, &meta));
        }
    }
    back
}

/// Removes the folders the app made for `file` under `base`, `base` included,
/// as long as they are empty.
fn remove_empty_dirs(file: &Path, base: &Path) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if !d.starts_with(base) || fs::remove_dir(d).is_err() {
            break;
        }
        if d == base {
            break;
        }
        dir = d.parent();
    }
}

/// The recorded moves whose file is still on the other drive. Files moved or
/// deleted by hand since are forgotten: there is nothing left to move back.
fn still_there(records: Vec<MovedFile>) -> Vec<MovedFile> {
    records
        .into_iter()
        .filter(|r| fs::symlink_metadata(&r.path).is_ok_and(|m| m.is_file()))
        .collect()
}

fn read_records(path: &Path) -> Vec<MovedFile> {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Written to a temporary file first, so a crash never leaves half a list.
fn write_records(path: &Path, records: &[MovedFile]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_vec_pretty(records).map_err(io::Error::other)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)
}

fn newest_first(mut records: Vec<MovedFile>) -> Vec<MovedFile> {
    records.sort_by(|a, b| b.moved_at.cmp(&a.moved_at).then(b.id.cmp(&a.id)));
    records
}

// ── Commands ────────────────────────────────────────────────────────────────

fn records_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|d| d.join(RECORDS))
        .map_err(|e| e.to_string())
}

/// The moves on record, tidied (see [`still_there`]).
fn current_records(path: &Path) -> Vec<MovedFile> {
    let all = read_records(path);
    let count = all.len();
    let kept = still_there(all);
    if kept.len() != count {
        let _ = write_records(path, &kept);
    }
    kept
}

fn snapshot(app: &AppHandle, kept: &Kept) -> Result<OffloadScan, String> {
    let records = current_records(&records_path(app)?);
    Ok(kept.snapshot(disk::other_drives(), newest_first(records)))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

/// Looks for files to make online only and files to move. Read only.
#[tauri::command]
pub async fn scan_offload(app: AppHandle) -> Result<OffloadScan, String> {
    blocking(move || {
        let found = find(
            &platform::places(),
            SystemTime::now(),
            &Rules::default(),
            &platform::attributes,
        );
        let state = app.state::<LastOffload>();
        let mut kept = state.lock();
        *kept = Kept {
            onedrive: found.onedrive,
            online: found.online,
            freeing_count: found.freeing_count,
            freeing_bytes: found.freeing_bytes,
            freed: Vec::new(),
            movable: found.movable,
        };
        snapshot(&app, &kept)
    })
    .await
}

/// The cards as they are now, without scanning again.
#[tauri::command]
pub async fn current_offload(app: AppHandle) -> Result<OffloadScan, String> {
    blocking(move || snapshot(&app, &app.state::<LastOffload>().lock())).await
}

/// "Free up space" on every scanned OneDrive file except `except`.
#[tauri::command]
pub async fn make_online_only(app: AppHandle, except: Vec<String>) -> Result<OnlineReport, String> {
    blocking(move || {
        let state = app.state::<LastOffload>();
        let mut kept = state.lock();
        let files = Kept::take(&mut kept.online, &except);
        let (report, done, left) = free_up(files, &platform::attributes, &platform::set_attributes);
        kept.online.extend(left);
        kept.online
            .sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
        kept.freeing_count += report.done_count;
        kept.freeing_bytes += report.done_bytes;
        kept.freed = done;
        Ok(report)
    })
    .await
}

/// Undoes the last "Free up space": the files are kept on this PC again.
#[tauri::command]
pub async fn keep_on_this_pc(app: AppHandle) -> Result<OnlineReport, String> {
    blocking(move || {
        let state = app.state::<LastOffload>();
        let mut kept = state.lock();
        let freed = std::mem::take(&mut kept.freed);
        let report = keep_again(&freed, &platform::attributes, &platform::set_attributes);
        kept.freeing_count = kept.freeing_count.saturating_sub(report.done_count);
        kept.freeing_bytes = kept.freeing_bytes.saturating_sub(report.done_bytes);
        kept.online.extend(freed);
        kept.online
            .sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
        Ok(report)
    })
    .await
}

/// Moves every scanned file except `except` to `drive`, into a folder named
/// `folder_name` (translated by the UI, e.g. "Moved from C").
#[tauri::command]
pub async fn move_to_drive(
    app: AppHandle,
    except: Vec<String>,
    drive: String,
    folder_name: String,
    on_progress: Channel<MoveProgress>,
) -> Result<MoveReport, String> {
    blocking(move || {
        let target = disk::other_drives()
            .into_iter()
            .find(|d| d.mount_point.eq_ignore_ascii_case(&drive))
            .ok_or("this drive is not available")?;
        let root = drive_root(&target.mount_point).ok_or("bad drive")?;
        if !valid_folder_name(&folder_name) {
            return Err("bad folder name".into());
        }
        let base = root.join(folder_name.trim());
        let path = records_path(&app)?;
        let state = app.state::<LastOffload>();
        let mut kept = state.lock();
        let files = Kept::take(&mut kept.movable, &except);
        let mut records = read_records(&path);
        let profile = platform::places().profile;
        let report = move_files(
            &files,
            profile.as_deref(),
            &base,
            target.free_bytes,
            now_secs(),
            &mut records,
            &mut |r| {
                let _ = write_records(&path, r);
            },
            &mut |p| {
                let _ = on_progress.send(p);
            },
        );
        let moved: HashSet<&str> = records
            .iter()
            .filter(|r| report.ids.contains(&r.id))
            .map(|r| r.original_path.as_str())
            .collect();
        let left: Vec<Seen> = files
            .iter()
            .filter(|s| !moved.contains(s.path.to_string_lossy().as_ref()))
            .cloned()
            .collect();
        kept.movable.extend(left);
        kept.movable
            .sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
        Ok(report)
    })
    .await
}

/// Moves files back from the other drive to where they were.
#[tauri::command]
pub async fn move_back_files(app: AppHandle, ids: Vec<String>) -> Result<MoveBackReport, String> {
    blocking(move || {
        let path = records_path(&app)?;
        let state = app.state::<LastOffload>();
        let mut kept = state.lock();
        let mut records = read_records(&path);
        let mut report = MoveBackReport::default();
        let back = move_back(&ids, &mut records, &mut report);
        write_records(&path, &records).map_err(|e| e.to_string())?;
        for seen in back {
            if is_movable_kind(&seen.path) {
                kept.movable.push(seen);
            }
        }
        kept.movable
            .sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
        Ok(report)
    })
    .await
}

/// Opens the folder of a moved file in Explorer, with the file selected.
#[tauri::command]
pub async fn show_moved_file(app: AppHandle, id: String) -> Result<(), String> {
    blocking(move || {
        let record = read_records(&records_path(&app)?)
            .into_iter()
            .find(|r| r.id == id)
            .ok_or("this file is not on record")?;
        crate::review::platform::reveal(Path::new(&record.path)).map_err(|e| e.to_string())
    })
    .await
}

#[cfg(windows)]
mod platform {
    use super::Places;
    use std::fs::Metadata;
    use std::io;
    use std::os::windows::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_FLAGS_AND_ATTRIBUTES};
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    pub fn attributes(_path: &Path, meta: &Metadata) -> u32 {
        meta.file_attributes()
    }

    pub fn set_attributes(path: &Path, attrs: u32) -> io::Result<()> {
        // SAFETY: plain call with a valid string.
        unsafe {
            SetFileAttributesW(
                &HSTRING::from(path.as_os_str()),
                FILE_FLAGS_AND_ATTRIBUTES(attrs),
            )
        }
        .map_err(|e| io::Error::other(e.message()))
    }

    fn env(name: &str) -> Option<PathBuf> {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    /// Personal and work OneDrive folders, and SharePoint libraries synced by OneDrive.
    fn onedrive_folders() -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
            .into_iter()
            .filter_map(env)
            .collect();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let lists = [
            (r"Software\Microsoft\OneDrive\Accounts", "UserFolder"),
            (r"Software\SyncEngines\Providers\OneDrive", "MountPoint"),
        ];
        for (list, value) in lists {
            let Ok(key) = hkcu.open_subkey(list) else {
                continue;
            };
            for sub in key.enum_keys().flatten() {
                if let Ok(folder) = key
                    .open_subkey(&sub)
                    .and_then(|k| k.get_value::<String, _>(value))
                {
                    if !folder.is_empty() {
                        out.push(PathBuf::from(folder));
                    }
                }
            }
        }
        out
    }

    pub fn places() -> Places {
        Places {
            onedrive: onedrive_folders(),
            profile: env("USERPROFILE"),
        }
    }
}

/// Easy Uninstaller targets Windows; elsewhere there is no OneDrive and nothing to move.
#[cfg(not(windows))]
mod platform {
    use super::Places;
    use std::fs::Metadata;
    use std::io;
    use std::path::Path;

    pub fn attributes(_path: &Path, _meta: &Metadata) -> u32 {
        0
    }

    pub fn set_attributes(_path: &Path, _attrs: u32) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    pub fn places() -> Places {
        Places::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::fs::{File, FileTimes};

    /// A fresh, empty folder under the system temp dir, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "easy-uninstaller-offload-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        /// Writes `size` bytes at `rel`, last changed `age_days` ago.
        fn file(&self, rel: &str, size: usize, age_days: u32) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, vec![7u8; size]).unwrap();
            let when = SystemTime::now() - DAY * age_days;
            let times = FileTimes::new().set_accessed(when).set_modified(when);
            File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_times(times)
                .unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn rules() -> Rules {
        Rules {
            online_min_bytes: 100,
            online_min_age: 30 * DAY,
            move_min_bytes: 1000,
            move_min_age: DAY,
        }
    }

    const PLACEHOLDER: u32 = FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_ARCHIVE;

    /// Pretends to be OneDrive: the file name says what state the file is in.
    fn fake_attributes(path: &Path, _meta: &Metadata) -> u32 {
        let name = path.file_name().unwrap().to_string_lossy();
        if name.starts_with("online") {
            PLACEHOLDER | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
        } else if name.starts_with("pending") {
            FILE_ATTRIBUTE_ARCHIVE
        } else if name.starts_with("freeing") {
            PLACEHOLDER | FILE_ATTRIBUTE_UNPINNED
        } else if name.starts_with("pinned") {
            PLACEHOLDER | FILE_ATTRIBUTE_PINNED
        } else {
            PLACEHOLDER
        }
    }

    fn names(files: &[Seen]) -> Vec<String> {
        files
            .iter()
            .map(|s| s.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn cloud_states_and_attributes() {
        assert_eq!(cloud_state(FILE_ATTRIBUTE_ARCHIVE), Cloud::NotSynced);
        assert_eq!(cloud_state(PLACEHOLDER), Cloud::Both);
        assert_eq!(
            cloud_state(PLACEHOLDER | FILE_ATTRIBUTE_PINNED),
            Cloud::Both
        );
        assert_eq!(
            cloud_state(PLACEHOLDER | FILE_ATTRIBUTE_UNPINNED),
            Cloud::Freeing
        );
        assert_eq!(
            cloud_state(PLACEHOLDER | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS),
            Cloud::OnlineOnly
        );
        assert_eq!(cloud_state(FILE_ATTRIBUTE_OFFLINE), Cloud::OnlineOnly);

        let pinned = PLACEHOLDER | FILE_ATTRIBUTE_PINNED | FILE_ATTRIBUTE_READONLY;
        let freed = online_only_attrs(pinned);
        assert_eq!(
            freed,
            FILE_ATTRIBUTE_ARCHIVE | FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_UNPINNED,
            "keeps the user's attributes, drops pinned and what Windows sets itself"
        );
        assert_eq!(
            keep_attrs(freed),
            FILE_ATTRIBUTE_ARCHIVE | FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_PINNED
        );
    }

    #[test]
    fn finds_onedrive_files_and_files_to_move() {
        let tmp = TempDir::new("find");
        let profile = tmp.0.join("Users").join("you");
        let onedrive = profile.join("OneDrive");
        let p = |rel: &str| format!("Users/you/{rel}");
        // OneDrive
        tmp.file(&p("OneDrive/Pictures/big.jpg"), 500, 60);
        tmp.file(&p("OneDrive/pinned.mp4"), 900, 400);
        tmp.file(&p("OneDrive/new.docx"), 500, 2);
        tmp.file(&p("OneDrive/tiny.txt"), 10, 90);
        tmp.file(&p("OneDrive/online.mov"), 800, 90);
        tmp.file(&p("OneDrive/pending.mp4"), 800, 90);
        tmp.file(&p("OneDrive/freeing.zip"), 300, 90);
        tmp.file(&p("OneDrive/Videos/in-onedrive.mp4"), 5000, 90);
        // To move
        tmp.file(&p("Videos/Holiday.mp4"), 3000, 10);
        tmp.file(&p("Downloads/Ubuntu.ISO"), 2000, 400);
        tmp.file(&p("Downloads/today.mp4"), 2000, 0);
        tmp.file(&p("Downloads/small.zip"), 10, 40);
        tmp.file(&p("Documents/mail.pst"), 9000, 40);
        tmp.file(&p("AppData/Local/cache.mp4"), 9000, 40);

        let places = Places {
            onedrive: vec![
                onedrive.clone(),
                onedrive.join("Pictures"),
                tmp.0.join("gone"),
            ],
            profile: Some(profile.clone()),
        };
        let found = find(&places, SystemTime::now(), &rules(), &fake_attributes);
        assert_eq!(found.onedrive, [onedrive]);
        assert_eq!(
            names(&found.online),
            ["in-onedrive.mp4", "pinned.mp4", "big.jpg"],
            "on the PC and on OneDrive, big enough, not changed lately; largest first"
        );
        assert_eq!((found.freeing_count, found.freeing_bytes), (1, 300));
        assert_eq!(
            names(&found.movable),
            ["Holiday.mp4", "Ubuntu.ISO"],
            "videos, archives and disk images; never OneDrive, AppData or other kinds"
        );
    }

    fn seen(path: &Path) -> Seen {
        Seen::from_meta(path.to_path_buf(), &fs::metadata(path).unwrap())
    }

    #[test]
    fn frees_up_space_and_undoes_it() {
        let tmp = TempDir::new("free");
        let a = tmp.file("a.mp4", 500, 60);
        let b = tmp.file("pinned.mp4", 400, 60);
        let changed = tmp.file("changed.mp4", 300, 60);
        let gone = tmp.file("gone.mp4", 200, 60);
        let files = vec![seen(&a), seen(&b), seen(&changed), seen(&gone)];
        fs::write(&changed, b"new content").unwrap();
        fs::remove_file(&gone).unwrap();

        let set: RefCell<Vec<(PathBuf, u32)>> = RefCell::default();
        let record = |p: &Path, attrs: u32| {
            set.borrow_mut().push((p.to_path_buf(), attrs));
            Ok(())
        };
        let (report, done, left) = free_up(files, &fake_attributes, &record);
        assert_eq!(
            report,
            OnlineReport {
                done_count: 2,
                done_bytes: 900,
                skipped_count: 2
            }
        );
        assert_eq!(names(&done), ["a.mp4", "pinned.mp4"]);
        assert_eq!(names(&left), ["changed.mp4", "gone.mp4"]);
        let unpinned = FILE_ATTRIBUTE_ARCHIVE | FILE_ATTRIBUTE_UNPINNED;
        assert_eq!(
            *set.borrow(),
            [(a.clone(), unpinned), (b.clone(), unpinned)]
        );

        set.borrow_mut().clear();
        let undo = keep_again(&done, &fake_attributes, &record);
        assert_eq!((undo.done_count, undo.done_bytes), (2, 900));
        let pinned = FILE_ATTRIBUTE_ARCHIVE | FILE_ATTRIBUTE_PINNED;
        assert_eq!(*set.borrow(), [(a, pinned), (b, pinned)]);

        let refused = |_: &Path, _: u32| Err(io::Error::other("no"));
        let (report, _, left) = free_up(
            vec![seen(&tmp.file("x.mp4", 50, 60))],
            &fake_attributes,
            &refused,
        );
        assert_eq!(
            (report.done_count, report.skipped_count, left.len()),
            (0, 1, 1)
        );
    }

    #[test]
    fn moves_files_and_moves_them_back() {
        let tmp = TempDir::new("move");
        let profile = tmp.0.join("you");
        let base = tmp.0.join("D").join("Moved from C");
        let video = tmp.file("you/Videos/Trip/Trip.mp4", 3000, 10);
        let iso = tmp.file("you/Downloads/disk.iso", 2000, 10);
        let changed = tmp.file("you/Downloads/changed.zip", 1500, 10);
        let elsewhere = tmp.file("elsewhere/song.flac", 1200, 10);
        let files = vec![seen(&video), seen(&iso), seen(&changed), seen(&elsewhere)];
        fs::write(&changed, b"new").unwrap();
        // Already a file with that name on the other drive.
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("song.flac"), b"someone else's").unwrap();

        let mut records = Vec::new();
        let saves = RefCell::new(0);
        let mut updates = Vec::new();
        let report = move_files(
            &files,
            Some(&profile),
            &base,
            u64::MAX / 2,
            1_800_000_000,
            &mut records,
            &mut |_| *saves.borrow_mut() += 1,
            &mut |p| updates.push(p),
        );
        assert_eq!((report.moved_count, report.moved_bytes), (3, 6200));
        assert_eq!((report.skipped_count, report.no_room_count), (1, 0));
        assert_eq!(*saves.borrow(), 3, "saved after every file");
        assert_eq!(updates.len(), 4);
        assert_eq!(updates[3].done_bytes, updates[3].total_bytes);
        assert!(!video.exists() && !iso.exists() && changed.exists());
        assert!(base.join("Videos").join("Trip").join("Trip.mp4").is_file());
        assert!(base.join("Downloads").join("disk.iso").is_file());
        assert!(base.join("song (2).flac").is_file(), "never overwrites");
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].original_path, video.to_string_lossy());
        assert_eq!(report.ids, ["1800000000-0", "1800000000-1", "1800000000-2"]);

        // The original name was taken meanwhile: it comes back renamed.
        fs::write(&iso, b"new download").unwrap();
        let mut back = MoveBackReport::default();
        let ids = vec![
            report.ids[0].clone(),
            report.ids[1].clone(),
            "nope".to_string(),
        ];
        let home = move_back(&ids, &mut records, &mut back);
        assert_eq!(
            back,
            MoveBackReport {
                restored_count: 2,
                restored_bytes: 5000,
                renamed_count: 1,
                failed_count: 1
            }
        );
        assert_eq!(names(&home), ["Trip.mp4", "disk (2).iso"]);
        assert!(video.is_file());
        assert!(
            !base.join("Videos").exists(),
            "empty folders made by the app go"
        );
        assert!(base.is_dir(), "song (2).flac is still there");
        assert_eq!(records.len(), 1);

        // Moved by hand on the other drive: forgotten.
        fs::remove_file(base.join("song (2).flac")).unwrap();
        assert!(still_there(records).is_empty());
    }

    #[test]
    fn leaves_room_on_the_other_drive() {
        let tmp = TempDir::new("room");
        let a = tmp.file("you/a.mp4", 3000, 10);
        let mut records = Vec::new();
        let report = move_files(
            &[seen(&a)],
            None,
            &tmp.0.join("D"),
            KEEP_FREE + 2999,
            1,
            &mut records,
            &mut |_| {},
            &mut |_| {},
        );
        assert_eq!((report.moved_count, report.no_room_count), (0, 1));
        assert!(a.is_file() && records.is_empty());
    }

    #[test]
    fn records_round_trip() {
        let tmp = TempDir::new("records");
        let path = tmp.0.join("data").join(RECORDS);
        assert!(read_records(&path).is_empty());
        let r = |id: &str, at: u64| MovedFile {
            id: id.into(),
            original_path: "C:\\a".into(),
            path: "D:\\a".into(),
            size_bytes: 1,
            moved_at: at,
            base: String::new(),
        };
        write_records(&path, &[r("1-0", 1), r("5-0", 5)]).unwrap();
        let read = newest_first(read_records(&path));
        assert_eq!(read[0].id, "5-0");
        assert_eq!(new_id(5, &read), "5-1");
    }

    #[test]
    fn names_and_targets() {
        assert!(valid_folder_name("Moved from C"));
        assert!(valid_folder_name("Spostati da C"));
        for bad in ["", "  ", "..", "a\\b", "a/b", "C:", "x?", "end."] {
            assert!(!valid_folder_name(bad), "{bad:?}");
        }
        assert!(is_under(
            Path::new("C:\\Users\\You\\OneDrive\\a.mp4"),
            Path::new("c:\\users\\you\\onedrive")
        ));
        assert!(is_under(
            Path::new("/u/OneDrive"),
            Path::new("/u/onedrive/")
        ));
        assert!(!is_under(
            Path::new("/u/OneDrive2/a"),
            Path::new("/u/OneDrive")
        ));
        assert_eq!(drive_root("d:"), Some(PathBuf::from("D:\\")));
        assert_eq!(drive_root("D:\\x"), None);
        assert_eq!(drive_root("DD"), None);

        let base = Path::new("/D/Moved");
        let profile = Path::new("/Users/you");
        assert_eq!(
            target_for(base, Some(profile), Path::new("/Users/you/Videos/a.mp4")),
            Path::new("/D/Moved/Videos/a.mp4")
        );
        assert_eq!(
            target_for(base, Some(profile), Path::new("/Other/b.zip")),
            Path::new("/D/Moved/b.zip")
        );
    }
}
