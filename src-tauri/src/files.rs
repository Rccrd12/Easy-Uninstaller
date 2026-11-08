//! Read-only scan for files the user may not need anymore, grouped by how safe
//! they are to remove. Nothing here deletes or changes a file: the full list of
//! each group is kept in [`LastScan`] so that `quarantine.rs` can move exactly
//! what was found (the UI only names groups, never paths).
//!
//! Groups, in the order they are claimed (a file lands in one group at most):
//! 1. `temp`: temporary files and app caches older than a day (Safe).
//! 2. `recycle-bin`: items in the Recycle Bin for over 30 days (Safe).
//! 3. `crash-reports`: error reports and crash dumps older than a day (Safe).
//! 4. `thumbnails`: the thumbnail cache Explorer rebuilds (Safe).
//! 5. `installers`: setup files in Downloads for programs that are installed (Safe).
//! 6. `old-downloads`: downloads not opened or changed in over a year (Check).
//! 7. `duplicates`: identical copies in the user's folders; the oldest copy is kept (Check).
//! 8. `large-files`: files of 1 GB or more in the user's folders (Check).
//! 9. `unknown-folders`: big folders in AppData and ProgramData (Check, review only:
//!    listed with an explanation, never moved as a group).
//!
//! Groups 2-4 and 9 live in `files/extra.rs`; folder explanations in `explain.rs`.
//!
//! What the UI shows is rebuilt from the kept file lists by `review.rs`, leaving
//! out files that are gone or changed and files the user asked never to see again.

mod extra;

use crate::explain::KnownFolders;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, Metadata};
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
/// Temp and cache files younger than this may be in use, so they are left out.
const TEMP_MIN_AGE: Duration = DAY;
/// A setup file this recent may be an update the user has not run yet.
const INSTALLER_MIN_AGE: Duration = DAY;
const OLD_DOWNLOAD_AGE: Duration = Duration::from_secs(365 * 24 * 60 * 60);
const LARGE_FILE_BYTES: u64 = 1024 * 1024 * 1024;
/// Smaller duplicates are not worth the time it takes to compare them.
const MIN_DUPLICATE_BYTES: u64 = 1024 * 1024;
/// How many items per group are sent to the UI (largest first); `item_count` has the full count.
pub(crate) const MAX_ITEMS: usize = 200;
/// Send a progress update every this many files.
const PROGRESS_EVERY: u64 = 250;
const INSTALLER_EXTENSIONS: [&str; 5] = ["exe", "msi", "msix", "msixbundle", "appx"];

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Safety {
    Safe,
    Check,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileItem {
    pub path: String,
    pub size_bytes: u64,
    /// Last time the file was used (opened or changed), as `YYYY-MM-DD`; `None` for folders.
    pub last_used: Option<String>,
    /// For duplicates: the copy that is kept.
    pub duplicate_of: Option<String>,
    /// A folder the app can explain ("What is this?"): an id from `known_folders.json`.
    pub explain_id: Option<String>,
    /// For Recycle Bin items: where the file was before it was deleted.
    pub original_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileGroup {
    pub id: String,
    /// i18n keys: the UI translates them.
    pub title_key: String,
    pub description_key: String,
    pub safety: Safety,
    pub size_bytes: u64,
    /// Number of files in the group.
    pub item_count: u64,
    /// Largest first, at most [`MAX_ITEMS`]. For temp files, one item per folder.
    pub items: Vec<FileItem>,
    /// Only shown and explained: it cannot be selected or moved as a whole.
    pub review_only: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScanStep {
    Caches,
    Downloads,
    Personal,
    Duplicates,
    AppFolders,
}

const STEPS: [ScanStep; 5] = [
    ScanStep::Caches,
    ScanStep::Downloads,
    ScanStep::Personal,
    ScanStep::Duplicates,
    ScanStep::AppFolders,
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub step: ScanStep,
    /// 1-based.
    pub step_index: u32,
    pub step_count: u32,
    pub files_checked: u64,
}

/// A file found by the scan, with what it looked like then, so a later move can
/// check that it is still the same file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
}

impl ScannedFile {
    fn new(path: PathBuf, meta: &Metadata) -> Self {
        ScannedFile {
            path,
            size: meta.len(),
            modified: meta.modified().unwrap_or(UNIX_EPOCH),
        }
    }
}

#[derive(Debug, Default)]
pub struct ScanResult {
    /// What the UI shows.
    pub groups: Vec<FileGroup>,
    /// Every file of each group, by group id (the UI only gets the largest ones).
    /// For `duplicates`, the extra copies (every set but its oldest file).
    pub files: HashMap<String, Vec<ScannedFile>>,
    /// Every set of identical files, oldest first: the copy kept unless the user picks another.
    pub duplicate_sets: Vec<Vec<ScannedFile>>,
}

/// The last scan, kept so that later moves act on exactly what was found, plus
/// the files and folders the user asked never to see again. A group can be moved
/// again after an undo: files already moved are simply not there anymore and are
/// skipped, and a file restored under a new name is not the scanned file.
#[derive(Default)]
pub struct LastScan(pub(crate) Mutex<Kept>);

#[derive(Debug, Default)]
pub(crate) struct Kept {
    /// The groups as scanned: their order, safety and, for folder groups like temp, the folders.
    pub groups: Vec<FileGroup>,
    pub files: HashMap<String, Vec<ScannedFile>>,
    pub duplicate_sets: Vec<Vec<ScannedFile>>,
    /// Paths of files and folders never to show or move; see `review.rs`.
    pub excluded: Vec<PathBuf>,
    /// Ids of whole groups the user hid.
    pub hidden_groups: Vec<String>,
}

impl LastScan {
    /// What moving a whole group takes: its files minus the excluded ones, and for
    /// duplicates every copy except the one each set keeps.
    pub fn group(&self, group_id: &str) -> Option<Vec<ScannedFile>> {
        crate::review::movable(&*self.0.lock().ok()?, group_id)
    }

    /// Keeps a new scan result and returns the groups to show.
    pub fn replace(&self, result: ScanResult) -> Vec<FileGroup> {
        let mut kept = self.0.lock().unwrap_or_else(|e| e.into_inner());
        kept.groups = result.groups;
        kept.files = result.files;
        kept.duplicate_sets = result.duplicate_sets;
        crate::review::current_groups(&kept)
    }
}

/// Where to look. Filled per platform by [`platform::locations`].
#[derive(Debug, Default)]
pub struct Locations {
    /// Temp and cache folders whose content apps recreate on their own.
    pub caches: Vec<PathBuf>,
    pub downloads: Option<PathBuf>,
    /// Roots searched for large files and duplicates (the user profile).
    pub personal: Vec<PathBuf>,
    /// The current user's Recycle Bin folder on each drive (`X:\$Recycle.Bin\<SID>`).
    pub recycle_bins: Vec<PathBuf>,
    /// Error report and crash dump folders.
    pub crash_reports: Vec<PathBuf>,
    /// Where Explorer keeps `thumbcache_*.db`.
    pub thumbnails: Option<PathBuf>,
    /// AppData and ProgramData: their big subfolders are listed and explained.
    pub app_data: Vec<PathBuf>,
    pub known: KnownFolders,
}

#[tauri::command]
pub async fn scan_files(
    app: AppHandle,
    on_progress: Channel<ScanProgress>,
) -> Result<Vec<FileGroup>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let locations = platform::locations();
        let installed = platform::installed_app_names();
        let result = scan(&locations, &installed, SystemTime::now(), &mut |p| {
            // The window may be gone; the scan result is still returned.
            let _ = on_progress.send(p);
        });
        app.state::<LastScan>().replace(result)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Temp and cache folders apps recreate on their own (also used by the space map).
pub fn cache_dirs() -> Vec<PathBuf> {
    platform::locations().caches
}

/// A file seen during the scan.
#[derive(Debug, Clone)]
pub(crate) struct Seen {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
    /// The later of last access and last change.
    pub last_used: SystemTime,
}

impl Seen {
    pub(crate) fn from_meta(path: PathBuf, meta: &Metadata) -> Self {
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        let accessed = meta.accessed().unwrap_or(UNIX_EPOCH);
        Seen {
            path,
            size: meta.len(),
            modified,
            last_used: modified.max(accessed),
        }
    }

    pub(crate) fn scanned(&self) -> ScannedFile {
        ScannedFile {
            path: self.path.clone(),
            size: self.size,
            modified: self.modified,
        }
    }

    pub(crate) fn item(&self, duplicate_of: Option<&Path>) -> FileItem {
        FileItem {
            path: self.path.to_string_lossy().into_owned(),
            size_bytes: self.size,
            last_used: Some(iso_date(self.last_used)),
            duplicate_of: duplicate_of.map(|p| p.to_string_lossy().into_owned()),
            explain_id: None,
            original_path: None,
        }
    }
}

struct Progress<'a> {
    report: &'a mut dyn FnMut(ScanProgress),
    step: ScanStep,
    files: u64,
}

impl Progress<'_> {
    fn start(&mut self, step: ScanStep) {
        self.step = step;
        self.files = 0;
        self.send();
    }

    fn tick(&mut self) {
        self.files += 1;
        if self.files.is_multiple_of(PROGRESS_EVERY) {
            self.send();
        }
    }

    fn send(&mut self) {
        let index = STEPS.iter().position(|s| *s == self.step).unwrap_or(0);
        (self.report)(ScanProgress {
            step: self.step,
            step_index: index as u32 + 1,
            step_count: STEPS.len() as u32,
            files_checked: self.files,
        });
    }
}

/// Runs the whole scan. `installed` holds the display names of installed programs.
pub fn scan(
    loc: &Locations,
    installed: &[String],
    now: SystemTime,
    report: &mut dyn FnMut(ScanProgress),
) -> ScanResult {
    let mut progress = Progress {
        report,
        step: STEPS[0],
        files: 0,
    };
    let older_than = |t: SystemTime, age: Duration| now.duration_since(t).is_ok_and(|d| d >= age);
    let mut groups = Vec::new();
    let mut files: HashMap<String, Vec<ScannedFile>> = HashMap::new();
    // Files already in a group, so that no file is counted twice.
    let mut claimed: HashSet<PathBuf> = HashSet::new();

    // 1. Temp files and caches: one item per folder.
    progress.start(ScanStep::Caches);
    let mut temp_items = Vec::new();
    let mut temp_files = Vec::new();
    for dir in dedup_dirs(&loc.caches) {
        let mut size = 0;
        walk(&dir, &|_| false, &mut |path, meta| {
            progress.tick();
            if older_than(meta.modified().unwrap_or(now), TEMP_MIN_AGE) {
                size += meta.len();
                claimed.insert(path.clone());
                temp_files.push(ScannedFile::new(path, meta));
            }
        });
        if size > 0 {
            temp_items.push(FileItem {
                path: dir.to_string_lossy().into_owned(),
                size_bytes: size,
                last_used: None,
                duplicate_of: None,
                explain_id: None,
                original_path: None,
            });
        }
    }
    groups.push(group(
        "temp",
        Safety::Safe,
        temp_files.len() as u64,
        temp_items,
    ));
    files.insert("temp".into(), temp_files);

    // 2, 3 and 4. Recycle Bin, error reports, thumbnails: one step with the caches.
    let bin = extra::recycle_bin(&loc.recycle_bins, now, &mut || progress.tick());
    let bin_files: Vec<ScannedFile> = bin.iter().flat_map(|e| e.files.clone()).collect();
    // Counted without the small `$I` info files, which the user never sees.
    let bin_count = bin_files.len() - bin.len();
    groups.push(group(
        "recycle-bin",
        Safety::Safe,
        bin_count as u64,
        bin.into_iter().map(|e| e.item).collect(),
    ));
    files.insert("recycle-bin".into(), bin_files);

    let crash = extra::crash_reports(
        &dedup_dirs(&loc.crash_reports),
        &|t| older_than(t, TEMP_MIN_AGE),
        &mut || progress.tick(),
    );
    let crash: Vec<Seen> = crash
        .into_iter()
        .filter(|s| !claimed.contains(&s.path))
        .collect();
    claimed.extend(crash.iter().map(|s| s.path.clone()));
    groups.push(files_group(
        "crash-reports",
        Safety::Safe,
        &crash,
        &mut files,
    ));

    let thumbs: Vec<Seen> = extra::thumbnails(loc.thumbnails.as_deref(), &mut || progress.tick())
        .into_iter()
        .filter(|s| !claimed.contains(&s.path))
        .collect();
    claimed.extend(thumbs.iter().map(|s| s.path.clone()));
    groups.push(files_group("thumbnails", Safety::Safe, &thumbs, &mut files));

    // 2 and 3. Downloads: installers of installed programs, then old files.
    progress.start(ScanStep::Downloads);
    let installed: Vec<Vec<String>> = installed.iter().map(|n| name_words(n)).collect();
    let mut installers = Vec::new();
    let mut old_downloads = Vec::new();
    if let Some(downloads) = &loc.downloads {
        walk(downloads, &|_| false, &mut |path, meta| {
            progress.tick();
            if claimed.contains(&path) {
                return;
            }
            let seen = Seen::from_meta(path, meta);
            if is_installer(&seen.path)
                && older_than(seen.modified, INSTALLER_MIN_AGE)
                && installer_matches(&seen.path, &installed)
            {
                installers.push(seen);
            } else if older_than(seen.last_used, OLD_DOWNLOAD_AGE) {
                old_downloads.push(seen);
            }
        });
    }
    claimed.extend(installers.iter().map(|s| s.path.clone()));
    claimed.extend(old_downloads.iter().map(|s| s.path.clone()));
    groups.push(files_group(
        "installers",
        Safety::Safe,
        &installers,
        &mut files,
    ));
    groups.push(files_group(
        "old-downloads",
        Safety::Check,
        &old_downloads,
        &mut files,
    ));

    // 4 and 5. The user's folders: duplicates, then large files.
    progress.start(ScanStep::Personal);
    let mut personal = Vec::new();
    for root in dedup_dirs(&loc.personal) {
        walk(&root, &skip_personal_dir, &mut |path, meta| {
            progress.tick();
            if meta.len() >= MIN_DUPLICATE_BYTES && !claimed.contains(&path) {
                personal.push(Seen::from_meta(path, meta));
            }
        });
    }

    progress.start(ScanStep::Duplicates);
    let duplicate_sets = find_duplicates(&personal, &mut || progress.tick());
    let mut dup_items: Vec<FileItem> = Vec::new();
    let mut dup_files = Vec::new();
    for set in &duplicate_sets {
        let (original, copies) = set.split_first().expect("a set has two files or more");
        claimed.insert(original.path.clone());
        for copy in copies {
            claimed.insert(copy.path.clone());
            dup_items.push(copy.item(Some(&original.path)));
            dup_files.push(copy.scanned());
        }
    }
    groups.push(group(
        "duplicates",
        Safety::Check,
        dup_files.len() as u64,
        dup_items,
    ));
    files.insert("duplicates".into(), dup_files);

    let large: Vec<Seen> = personal
        .into_iter()
        .filter(|s| s.size >= LARGE_FILE_BYTES && !claimed.contains(&s.path))
        .collect();
    groups.push(files_group(
        "large-files",
        Safety::Check,
        &large,
        &mut files,
    ));

    // 9. Big app folders: listed and explained only, so no files are kept for moving.
    progress.start(ScanStep::AppFolders);
    let folders = extra::app_folders(&dedup_dirs(&loc.app_data), &loc.known, &mut || {
        progress.tick()
    });
    let mut unknown = group(
        "unknown-folders",
        Safety::Check,
        folders.len() as u64,
        folders,
    );
    unknown.review_only = true;
    groups.push(unknown);

    groups.retain(|g| g.item_count > 0);
    files.retain(|id, _| groups.iter().any(|g| &g.id == id));
    for item in groups.iter_mut().flat_map(|g| g.items.iter_mut()) {
        if item.explain_id.is_none() {
            item.explain_id = loc
                .known
                .lookup(Path::new(&item.path))
                .map(|f| f.id.clone());
        }
    }
    let duplicate_sets = duplicate_sets
        .iter()
        .map(|set| set.iter().map(Seen::scanned).collect())
        .collect();
    ScanResult {
        groups,
        files,
        duplicate_sets,
    }
}

fn files_group(
    id: &str,
    safety: Safety,
    seen: &[Seen],
    files: &mut HashMap<String, Vec<ScannedFile>>,
) -> FileGroup {
    files.insert(id.into(), seen.iter().map(Seen::scanned).collect());
    let items = seen.iter().map(|s| s.item(None)).collect();
    group(id, safety, seen.len() as u64, items)
}

fn group(id: &str, safety: Safety, item_count: u64, mut items: Vec<FileItem>) -> FileGroup {
    let size_bytes = items.iter().map(|i| i.size_bytes).sum();
    items.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes).then(a.path.cmp(&b.path)));
    items.truncate(MAX_ITEMS);
    let key = to_camel(id);
    FileGroup {
        id: id.into(),
        title_key: format!("files.groups.{key}.title"),
        description_key: format!("files.groups.{key}.description"),
        safety,
        size_bytes,
        item_count,
        items,
        review_only: false,
    }
}

/// `old-downloads` -> `oldDownloads`, to match the i18n key style.
pub fn to_camel(id: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in id.chars() {
        if c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Drops repeated folders and folders inside another one in the list, so nothing is counted twice.
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

/// Walks `root` without following links, calling `on_file` for every regular file.
/// Folders for which `skip_dir` returns true are not entered. Unreadable entries are skipped.
fn walk(
    root: &Path,
    skip_dir: &dyn Fn(&Path) -> bool,
    on_file: &mut dyn FnMut(PathBuf, &Metadata),
) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if platform::skip(&meta) {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                if !skip_dir(&path) {
                    stack.push(path);
                }
            } else if kind.is_file() {
                on_file(path, &meta);
            }
        }
    }
}

/// In the user's folders, app data and developer folders are not personal files.
fn skip_personal_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.starts_with('.')
        || name.eq_ignore_ascii_case("AppData")
        || name.eq_ignore_ascii_case("node_modules")
}

fn is_installer(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        INSTALLER_EXTENSIONS
            .iter()
            .any(|x| e.eq_ignore_ascii_case(x))
    })
}

/// Words in setup file names that say nothing about which program it is.
const INSTALLER_NOISE: [&str; 22] = [
    "setup",
    "installer",
    "install",
    "x64",
    "x86",
    "win64",
    "win32",
    "win",
    "windows",
    "amd64",
    "arm64",
    "64bit",
    "32bit",
    "bit",
    "full",
    "online",
    "offline",
    "web",
    "standalone",
    "latest",
    "release",
    "stable",
];

/// The program name in a setup file name: `"ChromeSetup.exe"` -> `"chrome"`,
/// `"vlc-3.0.20-win64.exe"` -> `"vlc"`. Splits on punctuation and camelCase,
/// drops version numbers and words like "setup" or "x64".
fn installer_key(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut prev_lower = false;
    for c in stem.chars() {
        if !c.is_alphanumeric() {
            words.push(std::mem::take(&mut word));
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower {
            words.push(std::mem::take(&mut word));
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        word.extend(c.to_lowercase());
    }
    words.push(word);
    words
        .into_iter()
        .filter(|w| !w.is_empty())
        .filter(|w| !w.chars().all(|c| c.is_ascii_digit()))
        .filter(|w| !w.starts_with('v') || !w[1..].chars().all(|c| c.is_ascii_digit()))
        .filter(|w| !INSTALLER_NOISE.contains(&w.as_str()))
        .collect()
}

/// The words of an installed program's name, lowercase: `"Python 3.12 (64-bit)"` ->
/// `["python", "3", "12", "64", "bit"]`. Not split on camelCase, so "GitHub" stays one word.
fn name_words(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// True when the setup file's program name is one or more whole consecutive words of an
/// installed program's name: "ChromeSetup.exe" matches "Google Chrome", while
/// "Git-2.44.exe" does not match "GitHub Desktop". `installed` comes from [`name_words`].
fn installer_matches(path: &Path, installed: &[Vec<String>]) -> bool {
    let key = installer_key(path);
    // Too short to tell programs apart ("go", "qt").
    if key.chars().count() < 3 {
        return false;
    }
    installed.iter().any(|words| {
        (0..words.len()).any(|start| {
            let mut joined = String::new();
            for w in &words[start..] {
                joined.push_str(w);
                if joined.len() >= key.len() {
                    return joined == key;
                }
            }
            false
        })
    })
}

/// Finds identical files: same size, then same first 64 KB, then same full content.
/// Returns sets of two files or more, oldest first: the oldest is the copy kept.
fn find_duplicates(files: &[Seen], tick: &mut dyn FnMut()) -> Vec<Vec<Seen>> {
    let mut by_size: HashMap<u64, Vec<&Seen>> = HashMap::new();
    for f in files {
        by_size.entry(f.size).or_default().push(f);
    }
    // Two independently keyed hashes give a 128-bit fingerprint.
    let keys = (RandomState::new(), RandomState::new());
    let mut out = Vec::new();
    for same_size in by_size.into_values().filter(|v| v.len() > 1) {
        for same_head in split_by(
            &same_size,
            |f| fingerprint(&f.path, &keys, Some(64 * 1024)),
            tick,
        ) {
            for mut same in split_by(&same_head, |f| fingerprint(&f.path, &keys, None), tick) {
                same.sort_by(|a, b| a.modified.cmp(&b.modified).then(a.path.cmp(&b.path)));
                out.push(same.into_iter().cloned().collect());
            }
        }
    }
    out
}

/// Groups files by `key`, keeping only groups of two or more. Unreadable files are left out.
fn split_by<'a>(
    files: &[&'a Seen],
    key: impl Fn(&Seen) -> Option<(u64, u64)>,
    tick: &mut dyn FnMut(),
) -> Vec<Vec<&'a Seen>> {
    let mut groups: HashMap<(u64, u64), Vec<&'a Seen>> = HashMap::new();
    for f in files {
        tick();
        if let Some(k) = key(f) {
            groups.entry(k).or_default().push(*f);
        }
    }
    groups.into_values().filter(|g| g.len() > 1).collect()
}

/// Hashes a file's content (or its first `limit` bytes). Reads in fixed-size blocks
/// so identical files always feed the hashers the same way.
fn fingerprint(
    path: &Path,
    keys: &(RandomState, RandomState),
    limit: Option<u64>,
) -> Option<(u64, u64)> {
    let file = File::open(path).ok()?;
    let mut reader: Box<dyn Read> = match limit {
        Some(n) => Box::new(file.take(n)),
        None => Box::new(file),
    };
    let (mut a, mut b) = (keys.0.build_hasher(), keys.1.build_hasher());
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let mut filled = 0;
        while filled < buf.len() {
            match reader.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
        if filled == 0 {
            break;
        }
        a.write(&buf[..filled]);
        b.write(&buf[..filled]);
        if filled < buf.len() {
            break;
        }
    }
    Some((a.finish(), b.finish()))
}

/// `YYYY-MM-DD` in UTC.
pub(crate) fn iso_date(t: SystemTime) -> String {
    let days = t
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(windows)]
mod platform {
    use super::{KnownFolders, Locations};
    use std::fs::Metadata;
    use std::os::windows::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use winreg::enums::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
    };
    use winreg::RegKey;

    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x40000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x400000;

    /// Skips cloud-only files (OneDrive "online only": they use no disk space and
    /// reading them would download them) and hidden system files.
    pub fn skip(meta: &Metadata) -> bool {
        let a = meta.file_attributes();
        let hidden_system = FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM;
        let cloud_only = FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
        (a & hidden_system) == hidden_system || (a & cloud_only) != 0
    }

    fn env(name: &str) -> Option<PathBuf> {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    /// Subfolders of `dir` (e.g. browser profiles); empty when unreadable.
    fn subdirs(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn locations() -> Locations {
        let mut caches = Vec::new();
        caches.extend(env("TEMP"));
        caches.extend(env("SystemRoot").map(|r| r.join("Temp")));
        if let Some(local) = env("LOCALAPPDATA") {
            for sub in [
                r"Microsoft\Windows\INetCache",
                "D3DSCache",
                r"NVIDIA\DXCache",
                r"NVIDIA\GLCache",
            ] {
                caches.push(local.join(sub));
            }
            // Chromium browsers: every profile has its own caches.
            for browser in [
                r"Google\Chrome\User Data",
                r"Microsoft\Edge\User Data",
                r"BraveSoftware\Brave-Browser\User Data",
            ] {
                for profile in subdirs(&local.join(browser)) {
                    for cache in ["Cache", "Code Cache", "GPUCache"] {
                        caches.push(profile.join(cache));
                    }
                }
            }
            for profile in subdirs(&local.join(r"Mozilla\Firefox\Profiles")) {
                caches.push(profile.join("cache2"));
            }
        }
        caches.retain(|p| p.is_dir());

        let local = env("LOCALAPPDATA");
        // Only folders the user can clean without administrator rights; system-wide
        // reports (ProgramData, Windows\Minidump) would just be skipped when moved.
        let crash_reports = local
            .iter()
            .flat_map(|l| [l.join("CrashDumps"), l.join(r"Microsoft\Windows\WER")])
            .filter(|p| p.is_dir())
            .collect();
        let app_data = [env("LOCALAPPDATA"), env("APPDATA"), env("PROGRAMDATA")]
            .into_iter()
            .flatten()
            .filter(|p| p.is_dir())
            .collect();

        let profile = env("USERPROFILE");
        Locations {
            caches,
            downloads: downloads_dir().or_else(|| profile.as_ref().map(|p| p.join("Downloads"))),
            recycle_bins: profile.as_deref().map(recycle_bins).unwrap_or_default(),
            personal: profile.into_iter().collect(),
            crash_reports,
            thumbnails: local.map(|l| l.join(r"Microsoft\Windows\Explorer")),
            app_data,
            known: KnownFolders::from_env(),
        }
    }

    /// The user's Recycle Bin folder on each local drive. Other users' folders are left alone.
    fn recycle_bins(profile: &Path) -> Vec<PathBuf> {
        let Some(sid) = user_sid(profile) else {
            return Vec::new();
        };
        let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else {
            return Vec::new();
        };
        (&disks)
            .into_iter()
            .map(|d| d.mount_point().join("$Recycle.Bin").join(&sid))
            .filter(|p| p.is_dir())
            .collect()
    }

    /// The security id of the user whose profile folder is `profile` (e.g. `S-1-5-21-…`),
    /// which names their folder inside `$Recycle.Bin`.
    fn user_sid(profile: &Path) -> Option<String> {
        let list = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList")
            .ok()?;
        let wanted = profile.to_string_lossy().to_lowercase();
        list.enum_keys().flatten().find(|sid| {
            list.open_subkey(sid)
                .and_then(|k| k.get_value::<String, _>("ProfileImagePath"))
                .is_ok_and(|p| expand_env(&p).to_lowercase() == wanted)
        })
    }

    /// The Downloads folder, also when the user moved it to another place.
    fn downloads_dir() -> Option<PathBuf> {
        const DOWNLOADS: &str = "{374DE290-123F-4565-9164-39C4925E467B}";
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders")
            .ok()?;
        let raw: String = key.get_value(DOWNLOADS).ok()?;
        let path = PathBuf::from(expand_env(&raw));
        path.is_dir().then_some(path)
    }

    /// Expands `%VAR%` references.
    fn expand_env(s: &str) -> String {
        let mut out = String::new();
        let mut rest = s;
        while let Some(start) = rest.find('%') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            match after.find('%') {
                Some(end) => {
                    let name = &after[..end];
                    match std::env::var(name) {
                        Ok(v) => out.push_str(&v),
                        Err(_) => out.push_str(&rest[start..start + end + 2]),
                    }
                    rest = &after[end + 1..];
                }
                None => {
                    out.push_str(&rest[start..]);
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Display names of installed programs, from the same registry keys
    /// "Apps & features" reads. Store apps rarely ship as setup files, so they are not needed here.
    pub fn installed_app_names() -> Vec<String> {
        const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
        let roots = [
            (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_64KEY),
            (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_32KEY),
            (HKEY_CURRENT_USER, KEY_READ),
        ];
        let mut names = Vec::new();
        for (hive, flags) in roots {
            let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(UNINSTALL, flags) else {
                continue;
            };
            for sub in key.enum_keys().flatten() {
                if let Ok(app) = key.open_subkey_with_flags(&sub, flags) {
                    if let Ok(name) = app.get_value::<String, _>("DisplayName") {
                        names.push(name);
                    }
                }
            }
        }
        names
    }
}

/// Easy Uninstaller targets Windows; elsewhere there is nothing to scan.
#[cfg(not(windows))]
mod platform {
    use super::{KnownFolders, Locations};
    use std::fs::Metadata;

    pub fn skip(_meta: &Metadata) -> bool {
        false
    }

    pub fn locations() -> Locations {
        Locations {
            known: KnownFolders::from_env(),
            ..Locations::default()
        }
    }

    pub fn installed_app_names() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::FileTimes;

    /// A fresh, empty folder under the system temp dir, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "easy-uninstaller-test-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        /// Writes `size` bytes of `fill` at `rel`, last used `age_days` ago.
        fn file(&self, rel: &str, size: usize, fill: u8, age_days: u64) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, vec![fill; size]).unwrap();
            let when = SystemTime::now() - DAY * age_days as u32;
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

    const MB: usize = 1024 * 1024;

    fn run(loc: &Locations, installed: &[&str]) -> (ScanResult, Vec<ScanProgress>) {
        let installed: Vec<String> = installed.iter().map(|s| s.to_string()).collect();
        let mut updates = Vec::new();
        let result = scan(loc, &installed, SystemTime::now(), &mut |p| updates.push(p));
        (result, updates)
    }

    fn find<'a>(groups: &'a [FileGroup], id: &str) -> Option<&'a FileGroup> {
        groups.iter().find(|g| g.id == id)
    }

    #[test]
    fn groups_files_without_counting_any_twice() {
        let t = TempDir::new("groups");
        let cache = t.0.join("cache");
        let profile = t.0.join("profile");
        let downloads = profile.join("Downloads");

        t.file("cache/old.tmp", 1000, 1, 3);
        t.file("cache/sub/older.tmp", 500, 1, 10);
        t.file("cache/fresh.tmp", 999, 1, 0); // may be in use: left out
        t.file("profile/Downloads/ChromeSetup.exe", 2 * MB, 2, 5);
        t.file("profile/Downloads/UnknownToolSetup.exe", 2 * MB, 3, 5); // not installed
        t.file("profile/Downloads/vlc-3.0.20-win64.exe", 2 * MB, 4, 0); // too recent
        t.file("profile/Downloads/report-2019.pdf", 2 * MB, 5, 400);
        t.file("profile/Downloads/recent.pdf", 2 * MB, 6, 30);
        t.file("profile/Documents/photo.jpg", 3 * MB, 7, 100);
        t.file("profile/Pictures/photo copy.jpg", 3 * MB, 7, 10);
        t.file("profile/Pictures/other.jpg", 3 * MB, 8, 10); // same size, other content
        t.file("profile/Documents/report-2019.pdf", 2 * MB, 5, 1); // copy of an old download: already claimed
        t.file("profile/AppData/Local/big.bin", 2 * MB, 9, 1); // app data: not personal
        t.file("profile/AppData/Local/big copy.bin", 2 * MB, 9, 1);
        let movie = t.0.join("profile/Videos/movie.mkv");
        fs::create_dir_all(movie.parent().unwrap()).unwrap();
        File::create(&movie)
            .unwrap()
            .set_len(LARGE_FILE_BYTES)
            .unwrap(); // sparse: no real 1 GB written

        let loc = Locations {
            caches: vec![cache.clone(), cache.join("sub")],
            downloads: Some(downloads.clone()),
            personal: vec![profile.clone()],
            ..Default::default()
        };
        let (result, updates) = run(&loc, &["Google Chrome", "VLC media player"]);
        let groups = &result.groups;

        let temp = find(groups, "temp").unwrap();
        assert_eq!(temp.safety, Safety::Safe);
        assert_eq!((temp.item_count, temp.size_bytes), (2, 1500));
        assert_eq!(temp.items.len(), 1, "nested cache folders are merged");
        assert_eq!(temp.title_key, "files.groups.temp.title");

        let installers = find(groups, "installers").unwrap();
        assert_eq!(installers.item_count, 1);
        assert!(installers.items[0].path.ends_with("ChromeSetup.exe"));

        let old = find(groups, "old-downloads").unwrap();
        assert_eq!(old.safety, Safety::Check);
        assert_eq!(old.description_key, "files.groups.oldDownloads.description");
        assert_eq!(old.item_count, 1);
        assert!(old.items[0].path.ends_with("report-2019.pdf"));

        let dups = find(groups, "duplicates").unwrap();
        assert_eq!(dups.item_count, 1);
        let copy = &dups.items[0];
        assert!(
            copy.path.ends_with("photo copy.jpg"),
            "the older copy is kept"
        );
        assert!(copy.duplicate_of.as_deref().unwrap().ends_with("photo.jpg"));
        assert_eq!(copy.size_bytes, 3 * MB as u64);

        let large = find(groups, "large-files").unwrap();
        assert_eq!(large.item_count, 1);
        assert!(large.items[0].path.ends_with("movie.mkv"));
        assert_eq!(groups.len(), 5);

        // Every group keeps its full file list for the quarantine, temp files one by one.
        assert_eq!(result.files.len(), 5);
        for g in groups {
            let files = &result.files[&g.id];
            assert_eq!(files.len() as u64, g.item_count, "{}", g.id);
            assert_eq!(files.iter().map(|f| f.size).sum::<u64>(), g.size_bytes);
        }
        assert!(result.files["temp"]
            .iter()
            .any(|f| f.path.ends_with("older.tmp")));

        let steps: Vec<ScanStep> = updates.iter().map(|u| u.step).collect();
        assert_eq!(steps, STEPS);
        assert!(updates.iter().all(|u| u.step_count == 5));
    }

    #[test]
    fn missing_folders_give_no_groups() {
        let loc = Locations {
            caches: vec![PathBuf::from("/definitely/not/here")],
            downloads: Some(PathBuf::from("/nor/here")),
            personal: vec![],
            ..Default::default()
        };
        let (result, _) = run(&loc, &[]);
        assert!(result.groups.is_empty());
        assert!(result.files.is_empty());
    }

    #[test]
    fn installer_names() {
        let key = |s: &str| installer_key(Path::new(s));
        assert_eq!(key("ChromeSetup.exe"), "chrome");
        assert_eq!(key("vlc-3.0.20-win64.exe"), "vlc");
        assert_eq!(key("python-3.12.1-amd64.exe"), "python");
        assert_eq!(key("Firefox Installer.exe"), "firefox");
        assert_eq!(key("SpotifyFullSetup.exe"), "spotify");
        assert_eq!(key("Git-2.44.0-64-bit.exe"), "git");
        assert_eq!(key("node-v22.1.0-x64.msi"), "node");
        assert_eq!(key("setup.exe"), "");

        let installed: Vec<Vec<String>> = [
            "Google Chrome",
            "Python 3.12.1 (64-bit)",
            "GitHub Desktop",
            "Zoom Workplace",
            "Microsoft Edge",
            "OBS Studio",
        ]
        .iter()
        .map(|n| name_words(n))
        .collect();
        let matches = |s: &str| installer_matches(Path::new(s), &installed);
        assert!(matches("ChromeSetup.exe"));
        assert!(matches("python-3.12.1-amd64.exe"));
        assert!(matches("ZoomInstallerFull.exe"));
        assert!(matches("MicrosoftEdgeSetup.exe"));
        assert!(matches("OBS-Studio-30.1-Full-Installer-x64.exe"));
        assert!(
            !matches("Git-2.44.0-64-bit.exe"),
            "GitHub Desktop is not Git"
        );
        assert!(!matches("setup.exe"));
        assert!(!matches("FirefoxSetup.exe"));
    }

    #[test]
    fn camel_keys_and_dir_dedup() {
        assert_eq!(to_camel("old-downloads"), "oldDownloads");
        assert_eq!(to_camel("temp"), "temp");
        let dirs = [
            PathBuf::from("/a/b"),
            PathBuf::from("/a"),
            PathBuf::from("/a"),
            PathBuf::from("/c"),
        ];
        assert_eq!(
            dedup_dirs(&dirs),
            [PathBuf::from("/a"), PathBuf::from("/c")]
        );
    }

    #[test]
    fn iso_dates() {
        assert_eq!(iso_date(UNIX_EPOCH), "1970-01-01");
        let leap_day = UNIX_EPOCH + Duration::from_secs(951_782_400); // 2000-02-29
        assert_eq!(iso_date(leap_day), "2000-02-29");
        let later = UNIX_EPOCH + Duration::from_secs(1_793_491_200); // 2026-11-01
        assert_eq!(iso_date(later), "2026-11-01");
    }
}
