//! Quarantine: the only way Easy Uninstaller removes user files.
//!
//! Files are moved, never deleted, into a folder of the app
//! (`%LOCALAPPDATA%\com.rccrd12.easyuninstaller\Quarantine`), or into
//! `<drive>\Easy Uninstaller Quarantine` when the user picked another drive in
//! Settings (then moving a file off the system drive frees its space right away).
//! Each move makes one *entry*: a subfolder with the files and a `manifest.json`
//! that records where each file came from. An entry is kept for the number of days
//! chosen in Settings (3, 7 or 30), then deleted for good; until then it can be
//! restored to where it was, or deleted earlier on request.
//!
//! The only exception to "moved, never deleted" is temporary files and caches,
//! which `cleanup.rs` deletes right away (the owner's decision: apps recreate them).
//!
//! Changing the drive only affects new entries: entries already made stay where
//! they are, and every folder ever used is remembered so they are still listed,
//! restored and expired.
//!
//! The manifest is written before any file is moved and lists everything that is
//! about to move, so an interrupted move can still be restored: a listed file
//! that is not in the entry folder was simply never moved.

use crate::files::ScannedFile;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// How long removed items are kept, in days, until the user picks something else.
pub const DEFAULT_KEEP_DAYS: u64 = 7;
/// The choices offered in Settings.
pub const KEEP_DAYS_CHOICES: [u64; 3] = [3, 7, 30];
const DAY_SECS: u64 = 24 * 60 * 60;
const MANIFEST: &str = "manifest.json";
/// Stored in the default quarantine folder (a file, so never taken for an entry).
const SETTINGS: &str = "settings.json";
/// The quarantine folder on a drive picked by the user.
const DRIVE_FOLDER: &str = "Easy Uninstaller Quarantine";
/// How many files per entry are sent to the UI (largest first).
const MAX_ITEMS: usize = 50;

/// Where a moved file came from, and the name it has inside the entry folder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoredFile {
    original: String,
    stored: String,
    size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    version: u32,
    /// i18n key of the title (e.g. a file group), or...
    title_key: Option<String>,
    /// ...a plain title (e.g. an app name) when there is no key.
    title: Option<String>,
    /// Unix seconds.
    removed_at: u64,
    expires_at: u64,
    files: Vec<StoredFile>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuarantinedFile {
    pub original_path: String,
    pub size_bytes: u64,
}

/// One move into quarantine, as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineEntry {
    pub id: String,
    pub title_key: Option<String>,
    pub title: Option<String>,
    pub item_count: u64,
    pub size_bytes: u64,
    /// Unix seconds.
    pub removed_at: u64,
    pub expires_at: u64,
    /// Largest first, at most [`MAX_ITEMS`].
    pub items: Vec<QuarantinedFile>,
}

/// What removing files did. Filled by [`Quarantine::add`] and by `cleanup.rs`.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MoveReport {
    /// The entries created, one per group, so the UI can undo the move.
    pub entry_ids: Vec<String>,
    /// Moved into quarantine.
    pub moved_count: u64,
    pub moved_bytes: u64,
    /// Deleted right away (temporary files and caches only).
    pub deleted_count: u64,
    pub deleted_bytes: u64,
    /// Space really given back to the drives the files were on: what was deleted,
    /// plus what was moved to a quarantine on another drive. Files moved within
    /// their drive only free their space when the entry expires or is deleted.
    pub freed_bytes: u64,
    /// Files left in place: in use, changed since the scan, or gone.
    pub skipped_count: u64,
}

/// The quarantine choices the user makes in Settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineSettings {
    /// One of [`KEEP_DAYS_CHOICES`]. Applies to entries made from now on.
    pub keep_days: u64,
    /// The drive to keep the quarantine on (e.g. `"D:"`), or `None` for the
    /// app's own folder on the system drive.
    pub drive: Option<String>,
}

impl Default for QuarantineSettings {
    fn default() -> Self {
        QuarantineSettings {
            keep_days: DEFAULT_KEEP_DAYS,
            drive: None,
        }
    }
}

/// What is saved in [`SETTINGS`]: the choices, plus every other folder ever used.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
struct Stored {
    #[serde(flatten)]
    settings: QuarantineSettings,
    /// Folders on other drives that may still hold entries.
    other_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub restored_count: u64,
    /// Restored under a new name because something else now has the original name.
    pub renamed_count: u64,
    /// Still in quarantine (for example, the original folder cannot be written to).
    pub failed_count: u64,
}

/// The title of a new entry.
pub enum Title {
    Key(String),
    #[allow(dead_code)] // Used by the app uninstaller, coming next.
    Text(String),
}

/// The quarantine folders. All changes go through one lock, so two moves or a
/// move and a restore never touch the same entry at once.
pub struct Quarantine {
    /// The default folder, in the app's data folder. Also holds [`SETTINGS`].
    root: PathBuf,
    lock: Mutex<Stored>,
}

impl Quarantine {
    /// `root` is the default folder; saved settings are read from it.
    pub fn new(root: PathBuf) -> Self {
        let stored = fs::read(root.join(SETTINGS))
            .ok()
            .and_then(|text| serde_json::from_slice::<Stored>(&text).ok())
            .map(|mut s| {
                if !KEEP_DAYS_CHOICES.contains(&s.settings.keep_days) {
                    s.settings.keep_days = DEFAULT_KEEP_DAYS;
                }
                s
            })
            .unwrap_or_default();
        Quarantine {
            root,
            lock: Mutex::new(stored),
        }
    }

    fn guard(&self) -> MutexGuard<'_, Stored> {
        // A panic while holding the lock leaves nothing half done that the
        // manifest does not describe, so the lock is safe to reuse.
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn settings(&self) -> QuarantineSettings {
        self.guard().settings.clone()
    }

    /// Saves new choices. A drive is only accepted if its quarantine folder can be
    /// created; the folder it replaces is remembered, so its entries stay visible.
    pub fn set_settings(&self, new: QuarantineSettings) -> io::Result<()> {
        if !KEEP_DAYS_CHOICES.contains(&new.keep_days) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad keep_days"));
        }
        let mut stored = self.guard();
        if let Some(drive) = &new.drive {
            let root = drive_root(drive)?;
            fs::create_dir_all(&root)?;
            if !stored.other_roots.contains(&root) {
                stored.other_roots.push(root);
            }
        }
        let mut next = stored.clone();
        next.settings = new;
        fs::create_dir_all(&self.root)?;
        let text = serde_json::to_vec_pretty(&next).map_err(io::Error::other)?;
        let tmp = self.root.join("settings.tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, self.root.join(SETTINGS))?;
        *stored = next;
        Ok(())
    }

    /// Every folder that may hold entries: the default one first.
    fn roots(stored: &Stored, default: &Path) -> Vec<PathBuf> {
        let mut roots = vec![default.to_path_buf()];
        for r in &stored.other_roots {
            if !roots.contains(r) {
                roots.push(r.clone());
            }
        }
        roots
    }

    /// Where new entries go: the chosen drive if its folder can be made (the
    /// drive may be unplugged), otherwise the default folder.
    fn target_root(&self, stored: &Stored) -> PathBuf {
        stored
            .settings
            .drive
            .as_deref()
            .and_then(|d| drive_root(d).ok())
            .filter(|r| fs::create_dir_all(r).is_ok())
            .unwrap_or_else(|| self.root.clone())
    }

    /// Moves `files` into a new entry. Files that changed since the scan, are gone,
    /// or cannot be moved (in use, access denied) are left in place and counted as skipped.
    /// Returns the entry id, or `None` when nothing was moved.
    pub fn add(
        &self,
        title: Title,
        files: &[ScannedFile],
        now: u64,
        report: &mut MoveReport,
    ) -> io::Result<Option<String>> {
        let stored = self.guard();
        // Paths that are not valid Unicode cannot be written to the manifest.
        let (files, odd): (Vec<&ScannedFile>, Vec<&ScannedFile>) =
            files.iter().partition(|f| f.path.to_str().is_some());
        report.skipped_count += odd.len() as u64;
        if files.is_empty() {
            return Ok(None);
        }

        let roots = Self::roots(&stored, &self.root);
        let (id, dir) = new_entry_dir(&self.target_root(&stored), &roots, now, &title)?;
        let (title_key, title) = match title {
            Title::Key(k) => (Some(k), None),
            Title::Text(t) => (None, Some(t)),
        };
        let mut manifest = Manifest {
            version: 1,
            title_key,
            title,
            removed_at: now,
            expires_at: now + stored.settings.keep_days * DAY_SECS,
            files: files
                .iter()
                .enumerate()
                .map(|(i, f)| StoredFile {
                    original: f.path.to_string_lossy().into_owned(),
                    stored: stored_name(i, &f.path),
                    size: f.size,
                })
                .collect(),
        };
        // Written before anything moves: see the module docs.
        write_manifest(&dir, &manifest)?;

        let mut moved = Vec::new();
        for (file, entry) in files.iter().zip(&manifest.files) {
            let crossed = if unchanged_since_scan(file) {
                move_file(&file.path, &dir.join(&entry.stored)).ok()
            } else {
                None
            };
            match crossed {
                Some(crossed) => {
                    report.moved_count += 1;
                    report.moved_bytes += entry.size;
                    if crossed {
                        report.freed_bytes += entry.size;
                    }
                    moved.push(entry.clone());
                }
                None => report.skipped_count += 1,
            }
        }

        if moved.is_empty() {
            fs::remove_file(dir.join(MANIFEST))?;
            fs::remove_dir(&dir)?;
            return Ok(None);
        }
        manifest.files = moved;
        write_manifest(&dir, &manifest)?;
        report.entry_ids.push(id.clone());
        Ok(Some(id))
    }

    /// Every entry, newest first. Folders without a readable manifest are not ours
    /// to show or touch, and are left alone.
    pub fn list(&self) -> Vec<QuarantineEntry> {
        let stored = self.guard();
        let mut entries: Vec<QuarantineEntry> = manifests(&Self::roots(&stored, &self.root))
            .into_iter()
            .map(|(_, id, m)| entry(id, m))
            .collect();
        entries.sort_by(|a, b| b.removed_at.cmp(&a.removed_at).then(b.id.cmp(&a.id)));
        entries
    }

    /// Deletes for good the entries whose time is up. Returns how many were deleted.
    pub fn purge_expired(&self, now: u64) -> usize {
        let stored = self.guard();
        manifests(&Self::roots(&stored, &self.root))
            .into_iter()
            .filter(|(_, _, m)| m.expires_at <= now)
            .filter(|(dir, _, _)| fs::remove_dir_all(dir).is_ok())
            .count()
    }

    /// Moves an entry's files back where they were. When something else now has a
    /// file's name, the file comes back next to it as "name (restored).ext".
    /// Files that cannot be put back stay in quarantine.
    pub fn restore(&self, id: &str, report: &mut RestoreReport) -> io::Result<()> {
        let stored = self.guard();
        let dir = self.entry_dir(&stored, id)?;
        let mut manifest = read_manifest(&dir)?;
        let mut left = Vec::new();
        for file in manifest.files {
            let stored = dir.join(&file.stored);
            if !stored.exists() {
                // Never moved (the move was interrupted): nothing to put back.
                continue;
            }
            let original = PathBuf::from(&file.original);
            let target = free_name(&original);
            let restored = original
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| move_file(&stored, &target).map(|_| ()));
            match restored {
                Ok(()) => {
                    report.restored_count += 1;
                    if target != original {
                        report.renamed_count += 1;
                    }
                }
                Err(_) => {
                    report.failed_count += 1;
                    left.push(file);
                }
            }
        }
        if left.is_empty() {
            fs::remove_file(dir.join(MANIFEST))?;
            // Only fails if someone put other files in the folder: then it stays.
            let _ = fs::remove_dir(&dir);
        } else {
            manifest.files = left;
            write_manifest(&dir, &manifest)?;
        }
        Ok(())
    }

    /// Deletes an entry and its files for good.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        let stored = self.guard();
        let dir = self.entry_dir(&stored, id)?;
        read_manifest(&dir)?; // Only our own entries.
        fs::remove_dir_all(dir)
    }

    /// The folder of an existing entry, in whichever root holds it. Ids come from
    /// the UI, so anything that is not a plain entry name (like `..\..`) is refused.
    fn entry_dir(&self, stored: &Stored, id: &str) -> io::Result<PathBuf> {
        if !valid_id(id) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad entry id"));
        }
        Self::roots(stored, &self.root)
            .into_iter()
            .map(|r| r.join(id))
            .find(|d| d.is_dir())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "entry not found"))
    }
}

/// Every entry in `roots`, as (folder, id, manifest). Folders without a readable
/// manifest are not ours, and are left alone.
fn manifests(roots: &[PathBuf]) -> Vec<(PathBuf, String, Manifest)> {
    let mut out = Vec::new();
    for root in roots {
        let Ok(dirs) = fs::read_dir(root) else {
            continue; // Not made yet, or on a drive that is unplugged.
        };
        out.extend(
            dirs.flatten()
                .filter(|d| d.file_type().is_ok_and(|t| t.is_dir()))
                .filter_map(|d| {
                    let id = d.file_name().into_string().ok().filter(|id| valid_id(id))?;
                    let manifest = read_manifest(&d.path()).ok()?;
                    Some((d.path(), id, manifest))
                }),
        );
    }
    out
}

/// Makes a new entry folder in `target`, with an id no folder in `roots` has yet.
fn new_entry_dir(
    target: &Path,
    roots: &[PathBuf],
    now: u64,
    title: &Title,
) -> io::Result<(String, PathBuf)> {
    fs::create_dir_all(target)?;
    let slug = slug(match title {
        Title::Key(k) => k.split('.').nth(2).unwrap_or(k),
        Title::Text(t) => t,
    });
    for n in 0.. {
        let id = if n == 0 {
            format!("{now}-{slug}")
        } else {
            format!("{now}-{slug}-{n}")
        };
        if roots.iter().any(|r| r.join(&id).exists()) {
            continue;
        }
        let dir = target.join(&id);
        match fs::create_dir(&dir) {
            Ok(()) => return Ok((id, dir)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

/// `"D:"` -> `D:\Easy Uninstaller Quarantine`. Only plain drive letters are
/// accepted, since the value comes from the UI.
fn drive_root(drive: &str) -> io::Result<PathBuf> {
    let b = drive.trim_end_matches(['\\', '/']).as_bytes();
    if b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        Ok(PathBuf::from(format!("{}\\", &drive[..2])).join(DRIVE_FOLDER))
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, "bad drive"))
    }
}

fn entry(id: String, m: Manifest) -> QuarantineEntry {
    let mut items: Vec<QuarantinedFile> = m
        .files
        .iter()
        .map(|f| QuarantinedFile {
            original_path: f.original.clone(),
            size_bytes: f.size,
        })
        .collect();
    items.sort_by(|a, b| {
        b.size_bytes
            .cmp(&a.size_bytes)
            .then(a.original_path.cmp(&b.original_path))
    });
    items.truncate(MAX_ITEMS);
    QuarantineEntry {
        id,
        title_key: m.title_key,
        title: m.title,
        item_count: m.files.len() as u64,
        size_bytes: m.files.iter().map(|f| f.size).sum(),
        removed_at: m.removed_at,
        expires_at: m.expires_at,
        items,
    }
}

/// Entry ids are made by [`Quarantine::new_entry_dir`]: digits, lowercase letters and dashes.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || c == '-')
}

/// `"Old downloads"` -> `"old-downloads"`.
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 30 {
            break;
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "items".into()
    } else {
        out.into()
    }
}

/// The name a file gets inside its entry folder: a number keeps names unique,
/// the original name helps anyone who opens the folder.
fn stored_name(index: usize, path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let short: String = name
        .chars()
        .rev()
        .take(60)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{index} {short}")
}

/// True when the file is still the regular file the scan saw: same size, same
/// last change. Anything else is left alone.
pub fn unchanged_since_scan(file: &ScannedFile) -> bool {
    fs::symlink_metadata(&file.path).is_ok_and(|m| {
        m.is_file() && m.len() == file.size && m.modified().is_ok_and(|t| t == file.modified)
    })
}

/// `path` if it is free, otherwise `name (restored).ext`, `name (restored 2).ext`, ...
fn free_name(path: &Path) -> PathBuf {
    if fs::symlink_metadata(path).is_err() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map_or("".into(), |s| s.to_string_lossy());
    let ext = path
        .extension()
        .map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));
    (1..)
        .map(|n| {
            let suffix = if n == 1 {
                "(restored)".to_string()
            } else {
                format!("(restored {n})")
            };
            path.with_file_name(format!("{stem} {suffix}{ext}"))
        })
        .find(|p| fs::symlink_metadata(p).is_err())
        .expect("some name is free")
}

/// Moves a file. Within a drive this is a rename (instant, nothing is copied).
/// Across drives the file is copied, then the original removed; if the original
/// cannot be removed the copy is dropped, so a file never ends up in two places.
/// Returns true when the file changed drive (so its old drive got the space back).
fn move_file(from: &Path, to: &Path) -> io::Result<bool> {
    if fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "target exists",
        ));
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(false),
        Err(e) if crosses_drives(&e) => {
            if let Err(e) = fs::copy(from, to) {
                let _ = fs::remove_file(to);
                return Err(e);
            }
            if let Err(e) = fs::remove_file(from) {
                let _ = fs::remove_file(to);
                return Err(e);
            }
            Ok(true)
        }
        Err(e) => Err(e),
    }
}

fn crosses_drives(e: &io::Error) -> bool {
    // ERROR_NOT_SAME_DEVICE on Windows, EXDEV elsewhere.
    const CODE: i32 = if cfg!(windows) { 17 } else { 18 };
    e.raw_os_error() == Some(CODE)
}

fn read_manifest(dir: &Path) -> io::Result<Manifest> {
    let text = fs::read(dir.join(MANIFEST))?;
    serde_json::from_slice(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Writes the manifest to a temporary file first, so a crash never leaves half a manifest.
fn write_manifest(dir: &Path, manifest: &Manifest) -> io::Result<()> {
    let text = serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?;
    let tmp = dir.join("manifest.tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, dir.join(MANIFEST))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Sets up the quarantine folder and, in the background, deletes expired entries.
pub fn init(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let root = app.path().app_local_data_dir()?.join("Quarantine");
    app.manage(Quarantine::new(root));
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Quarantine>().purge_expired(now_secs());
    });
    Ok(())
}

/// The quarantine choices, plus the drives it could be moved to.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineOptions {
    pub settings: QuarantineSettings,
    /// The drive Windows is on, where the default quarantine folder is.
    pub system_drive: String,
    /// Other fixed drives, largest free space first.
    pub drives: Vec<crate::disk::DriveInfo>,
}

#[tauri::command]
pub async fn get_quarantine_options(app: AppHandle) -> Result<QuarantineOptions, String> {
    tauri::async_runtime::spawn_blocking(move || QuarantineOptions {
        settings: app.state::<Quarantine>().settings(),
        system_drive: crate::disk::system_drive(),
        drives: crate::disk::other_drives(),
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_quarantine_settings(
    app: AppHandle,
    settings: QuarantineSettings,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Quarantine>()
            .set_settings(settings)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_quarantine(app: AppHandle) -> Result<Vec<QuarantineEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let quarantine = app.state::<Quarantine>();
        quarantine.purge_expired(now_secs());
        quarantine.list()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restore_quarantine(app: AppHandle, ids: Vec<String>) -> Result<RestoreReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let quarantine = app.state::<Quarantine>();
        let mut report = RestoreReport::default();
        for id in ids {
            quarantine
                .restore(&id, &mut report)
                .map_err(|e| e.to_string())?;
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_quarantine(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Quarantine>()
            .delete(&id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder with a `home` (the user's files) and a `q` (the quarantine).
    struct Sandbox(PathBuf);

    impl Sandbox {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("easy-uninstaller-q-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("home")).unwrap();
            Sandbox(dir)
        }

        fn quarantine(&self) -> Quarantine {
            Quarantine::new(self.0.join("q"))
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

    fn key(k: &str) -> Title {
        Title::Key(format!("files.groups.{k}.title"))
    }

    #[test]
    fn moves_lists_and_restores() {
        let s = Sandbox::new("roundtrip");
        let q = s.quarantine();
        let a = s.file("Downloads/setup.exe", "aaaa");
        let b = s.file("Downloads/sub/old.pdf", "bb");
        let mut report = MoveReport::default();
        let id = q
            .add(key("installers"), &[a.clone(), b.clone()], NOW, &mut report)
            .unwrap()
            .unwrap();
        assert_eq!(id, format!("{NOW}-installers"));
        assert_eq!((report.moved_count, report.moved_bytes), (2, 6));
        assert_eq!(report.entry_ids, std::slice::from_ref(&id));
        assert!(!a.path.exists() && !b.path.exists(), "moved, not copied");

        let list = q.list();
        assert_eq!(list.len(), 1);
        let e = &list[0];
        assert_eq!(
            e.title_key.as_deref(),
            Some("files.groups.installers.title")
        );
        assert_eq!((e.item_count, e.size_bytes), (2, 6));
        assert_eq!(e.expires_at, NOW + 7 * DAY_SECS);
        assert_eq!(
            e.items[0].original_path,
            a.path.to_str().unwrap(),
            "largest first"
        );

        // The original folder is gone, and something new took a file's name.
        fs::remove_dir_all(b.path.parent().unwrap()).unwrap();
        fs::write(&a.path, "new").unwrap();
        let mut restored = RestoreReport::default();
        q.restore(&id, &mut restored).unwrap();
        assert_eq!(
            restored,
            RestoreReport {
                restored_count: 2,
                renamed_count: 1,
                failed_count: 0
            }
        );
        assert_eq!(fs::read_to_string(&b.path).unwrap(), "bb");
        assert_eq!(
            fs::read_to_string(&a.path).unwrap(),
            "new",
            "never overwritten"
        );
        let renamed = a.path.with_file_name("setup (restored).exe");
        assert_eq!(fs::read_to_string(renamed).unwrap(), "aaaa");
        assert!(q.list().is_empty());
        assert!(!s.0.join("q").join(&id).exists());
    }

    #[test]
    fn leaves_changed_and_missing_files_in_place() {
        let s = Sandbox::new("skip");
        let q = s.quarantine();
        let changed = s.file("a.txt", "one");
        fs::write(&changed.path, "changed!").unwrap();
        let gone = s.file("b.txt", "two");
        fs::remove_file(&gone.path).unwrap();
        let mut report = MoveReport::default();
        let id = q.add(key("temp"), &[changed.clone(), gone], NOW, &mut report);
        assert_eq!(id.unwrap(), None, "nothing moved, no entry");
        assert_eq!((report.moved_count, report.skipped_count), (0, 2));
        assert!(changed.path.exists());
        assert!(q.list().is_empty());
        assert_eq!(fs::read_dir(s.0.join("q")).unwrap().count(), 0);
    }

    #[test]
    fn restores_an_interrupted_move() {
        let s = Sandbox::new("interrupted");
        let q = s.quarantine();
        let a = s.file("a.txt", "a");
        let b = s.file("b.txt", "b");
        let mut report = MoveReport::default();
        let id = q
            .add(key("temp"), &[a.clone(), b.clone()], NOW, &mut report)
            .unwrap()
            .unwrap();
        // As if the app stopped after moving only `a`: `b` is listed but still at home.
        let dir = s.0.join("q").join(&id);
        let mut m = read_manifest(&dir).unwrap();
        let stored_b = m
            .files
            .iter()
            .find(|f| f.original.ends_with("b.txt"))
            .unwrap();
        fs::rename(dir.join(&stored_b.stored), &b.path).unwrap();
        m.files.iter_mut().for_each(|f| f.size = 1);
        write_manifest(&dir, &m).unwrap();

        let mut restored = RestoreReport::default();
        q.restore(&id, &mut restored).unwrap();
        assert_eq!((restored.restored_count, restored.renamed_count), (1, 0));
        assert_eq!(fs::read_to_string(&a.path).unwrap(), "a");
        assert_eq!(fs::read_to_string(&b.path).unwrap(), "b");
        assert!(!dir.exists());
    }

    #[test]
    fn expires_and_deletes() {
        let s = Sandbox::new("expire");
        let q = s.quarantine();
        let mut report = MoveReport::default();
        let old = q
            .add(key("temp"), &[s.file("old.tmp", "x")], NOW, &mut report)
            .unwrap()
            .unwrap();
        let newer = q
            .add(
                key("temp"),
                &[s.file("new.tmp", "y")],
                NOW + DAY_SECS,
                &mut report,
            )
            .unwrap()
            .unwrap();
        let other = q
            .add(
                key("temp"),
                &[s.file("other.tmp", "z")],
                NOW + DAY_SECS,
                &mut report,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            other,
            format!("{}-temp-1", NOW + DAY_SECS),
            "ids never clash"
        );
        // Not ours: no manifest. Must survive everything.
        fs::create_dir_all(s.0.join("q").join("stranger")).unwrap();

        assert_eq!(q.purge_expired(NOW + 7 * DAY_SECS - 1), 0);
        assert_eq!(q.purge_expired(NOW + 7 * DAY_SECS), 1);
        assert!(!s.0.join("q").join(&old).exists());
        let ids: Vec<String> = q.list().into_iter().map(|e| e.id).collect();
        assert_eq!(ids, [other.clone(), newer.clone()], "newest first");

        q.delete(&newer).unwrap();
        assert_eq!(q.list().len(), 1);
        assert!(q.delete("stranger").is_err());
        assert!(s.0.join("q").join("stranger").exists());
    }

    #[test]
    fn refuses_ids_that_leave_the_quarantine() {
        let s = Sandbox::new("ids");
        let q = s.quarantine();
        let mut report = RestoreReport::default();
        for bad in ["..", "../home", "a/b", r"a\b", "", "C:", "UPPER"] {
            assert!(q.delete(bad).is_err(), "{bad}");
            assert!(q.restore(bad, &mut report).is_err(), "{bad}");
        }
        assert!(s.0.join("home").exists());
    }

    #[test]
    fn keeps_for_the_chosen_days_and_remembers_it() {
        let s = Sandbox::new("days");
        let q = s.quarantine();
        assert_eq!(q.settings(), QuarantineSettings::default());
        for bad in [0, 5, 365] {
            let bad = QuarantineSettings {
                keep_days: bad,
                drive: None,
            };
            assert!(q.set_settings(bad).is_err());
        }
        let month = QuarantineSettings {
            keep_days: 30,
            drive: None,
        };
        q.set_settings(month.clone()).unwrap();

        let reopened = s.quarantine();
        assert_eq!(reopened.settings(), month, "saved for the next start");
        let mut report = MoveReport::default();
        reopened
            .add(key("temp"), &[s.file("a.tmp", "a")], NOW, &mut report)
            .unwrap();
        let list = reopened.list();
        assert_eq!(list.len(), 1, "the settings file is not an entry");
        assert_eq!(list[0].expires_at, NOW + 30 * DAY_SECS);
        assert_eq!(reopened.purge_expired(NOW + 29 * DAY_SECS), 0);
    }

    #[test]
    fn refuses_drives_that_are_not_drive_letters() {
        let s = Sandbox::new("drives");
        let q = s.quarantine();
        for bad in ["", "D", "..", r"C:\Windows", "/tmp", "DD:"] {
            let settings = QuarantineSettings {
                keep_days: 7,
                drive: Some(bad.into()),
            };
            assert!(q.set_settings(settings).is_err(), "{bad}");
        }
        assert!(drive_root("D:").is_ok() && drive_root(r"e:\").is_ok());
        assert_eq!(q.settings(), QuarantineSettings::default());
    }

    #[cfg(windows)]
    #[test]
    fn drive_folder() {
        assert_eq!(
            drive_root("D:").unwrap(),
            PathBuf::from(r"D:\Easy Uninstaller Quarantine")
        );
    }

    #[test]
    fn finds_entries_in_every_folder_used() {
        let s = Sandbox::new("roots");
        let q = s.quarantine();
        let other = s.0.join("other-drive");
        q.guard().other_roots.push(other.clone());
        // An entry made while the quarantine was on the other drive.
        let a = s.file("a.txt", "a");
        let roots = vec![s.0.join("q"), other.clone()];
        let (id, dir) = new_entry_dir(&other, &roots, NOW, &key("installers")).unwrap();
        let stored = stored_name(0, &a.path);
        fs::rename(&a.path, dir.join(&stored)).unwrap();
        let m = Manifest {
            version: 1,
            title_key: Some("files.groups.installers.title".into()),
            title: None,
            removed_at: NOW,
            expires_at: NOW + DAY_SECS,
            files: vec![StoredFile {
                original: a.path.to_string_lossy().into_owned(),
                stored,
                size: 1,
            }],
        };
        write_manifest(&dir, &m).unwrap();

        // A new entry made in the same second in the default folder gets another id.
        let mut report = MoveReport::default();
        let id2 = q
            .add(key("installers"), &[s.file("b.txt", "b")], NOW, &mut report)
            .unwrap()
            .unwrap();
        assert_ne!(id, id2, "ids never clash across folders");
        assert!(s.0.join("q").join(&id2).is_dir());

        let ids: Vec<String> = q.list().into_iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id));

        let mut restored = RestoreReport::default();
        q.restore(&id, &mut restored).unwrap();
        assert_eq!(restored.restored_count, 1);
        assert_eq!(fs::read_to_string(&a.path).unwrap(), "a");
        assert!(!dir.exists());

        // Unplugged drive: its folder is simply skipped.
        fs::remove_dir_all(&other).unwrap();
        assert_eq!(q.list().len(), 1);
        assert_eq!(q.purge_expired(NOW + 30 * DAY_SECS), 1);
    }

    #[test]
    fn names() {
        assert_eq!(slug("Old downloads!"), "old-downloads");
        assert_eq!(slug("Ünïcode"), "n-code");
        assert_eq!(slug("***"), "items");
        assert_eq!(stored_name(3, Path::new("/x/photo.jpg")), "3 photo.jpg");
        assert!(valid_id(&format!("{NOW}-large-files-2")));
        assert!(!valid_id("../x"));
    }
}
