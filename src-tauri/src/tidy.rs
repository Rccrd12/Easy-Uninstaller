//! Tidying a folder (AI help, phase 3): Downloads unless the user picks another.
//!
//! The files lying loose in the folder are grouped into a few folders with clear
//! names. The AI proposes the folders from the file names (only names, sizes and
//! months are sent, never contents); without the AI they are grouped by type.
//! Either way it is only a proposal: the user sees every folder and every file,
//! unticks what they want, and nothing moves until they press the button.
//!
//! What keeps it safe:
//! - only files listed in the proposal move, and only if unchanged since;
//! - files are moved, not removed: they go into new folders inside the folder
//!   being tidied (from Downloads and the Desktop also into Documents, Pictures,
//!   Videos or Music), never anywhere else, and a file of the same name is never replaced;
//! - system and program folders cannot be picked (see [`refused`]);
//! - every tidy is recorded, so it can be undone: files go back where they were
//!   and the folders it made are removed when empty;
//! - setup files of programs already installed are offered for the quarantine
//!   (through [`cleanup::remove`]), never ticked by default.

use crate::ai::prompt;
use crate::ai::AiConfig;
use crate::cleanup;
use crate::files::{self, iso_date, ScannedFile, Seen};
use crate::quarantine::{move_file, unchanged_since_scan, MoveReport, Quarantine, DRIVE_FOLDER};
use crate::search::{kind_of, FileKind, Place};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// At most this many file names are sent; the others stay for the next time.
const MAX_SENT: usize = 300;
const MAX_FOLDERS: usize = 12;
const MAX_FOLDER_CHARS: usize = 50;
/// A file changed this recently may still be downloading.
const RECENT: Duration = Duration::from_secs(10 * 60);
/// Shortcuts and links: they point elsewhere and are left where the user put them.
const LINKS: [&str; 2] = ["lnk", "url"];
/// Downloads in progress, by browser and program.
const PARTIAL: [&str; 9] = [
    "crdownload",
    "part",
    "partial",
    "download",
    "opdownload",
    "tmp",
    "!ut",
    "aria2",
    "!qb",
];
/// How many tidies are remembered for undo.
const KEEP_RUNS: usize = 20;
/// Group id of the setup files moved to the quarantine (titles the entry).
const INSTALLERS_GROUP: &str = "installers";

/// Where a proposed folder goes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Dest {
    /// Inside the folder being tidied.
    Here,
    Documents,
    Pictures,
    Videos,
    Music,
}

impl Dest {
    fn from_id(id: &str) -> Self {
        match id {
            "documents" => Dest::Documents,
            "pictures" => Dest::Pictures,
            "videos" => Dest::Videos,
            "music" => Dest::Music,
            _ => Dest::Here,
        }
    }

    /// Photos go to Pictures, videos to Videos, music to Music; anything to Documents or Downloads.
    fn allows(self, kind: Option<FileKind>) -> bool {
        match self {
            Dest::Pictures => matches!(kind, Some(FileKind::Image | FileKind::Video)),
            Dest::Videos => kind == Some(FileKind::Video),
            Dest::Music => kind == Some(FileKind::Audio),
            Dest::Documents | Dest::Here => true,
        }
    }
}

/// The folder being tidied and the folders a proposal may put files in.
#[derive(Debug, Clone, Default)]
pub struct Dirs {
    pub here: PathBuf,
    pub documents: Option<PathBuf>,
    pub pictures: Option<PathBuf>,
    pub videos: Option<PathBuf>,
    pub music: Option<PathBuf>,
}

impl Dirs {
    /// `here` alone: every proposed folder is made inside it.
    fn only(here: PathBuf) -> Self {
        Dirs {
            here,
            ..Dirs::default()
        }
    }

    /// `here` plus the user's Documents, Pictures, Videos and Music.
    fn with_libraries(here: PathBuf) -> Self {
        Dirs {
            here,
            documents: Place::Documents.dir(),
            pictures: Place::Pictures.dir(),
            videos: Place::Videos.dir(),
            music: Place::Music.dir(),
        }
    }

    fn libraries(&self) -> bool {
        self.documents.is_some()
            || self.pictures.is_some()
            || self.videos.is_some()
            || self.music.is_some()
    }

    /// The folder for `dest`; the folder being tidied when that one is not offered.
    fn get(&self, dest: Dest) -> (Dest, &Path) {
        let other = match dest {
            Dest::Here => None,
            Dest::Documents => self.documents.as_deref(),
            Dest::Pictures => self.pictures.as_deref(),
            Dest::Videos => self.videos.as_deref(),
            Dest::Music => self.music.as_deref(),
        };
        match other {
            Some(dir) => (dest, dir),
            None => (Dest::Here, &self.here),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TidyFile {
    pub path: String,
    pub size_bytes: u64,
    /// Last change, as `YYYY-MM-DD`.
    pub modified: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TidyFolder {
    pub id: String,
    pub name: String,
    pub dest: Dest,
    /// A few words on what is inside (from the AI), or "" for folders by type.
    pub why: String,
    /// The folder's full path.
    pub target: String,
    /// It is already there: files are added to it.
    pub exists: bool,
    pub files: Vec<TidyFile>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TidyPlan {
    pub folders: Vec<TidyFolder>,
    /// Setup files of programs already installed: offered for the quarantine.
    pub installers: Vec<TidyFile>,
    /// Loose files no folder took: they stay where they are.
    pub left_count: u64,
    /// Files not sent this time (over [`MAX_SENT`]).
    pub not_sent_count: u64,
    pub by_ai: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TidyRunInfo {
    pub id: String,
    pub at: u64,
    /// The folder that was tidied.
    pub folder: String,
    pub moved_count: u64,
    pub folder_count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TidyOverview {
    /// The folder to tidy; `None` when Downloads was asked for and there is none.
    pub folder: Option<String>,
    /// Folders offered with one click: Downloads, Desktop, Documents.
    pub places: Vec<TidyPlace>,
    /// Loose files that can be tidied.
    pub file_count: u64,
    pub size_bytes: u64,
    /// The last tidy that can still be undone.
    pub last_run: Option<TidyRunInfo>,
}

/// A folder offered with one click.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TidyPlace {
    /// `downloads`, `desktop` or `documents`.
    pub id: String,
    pub path: String,
}

/// What would be sent to the AI, shown before sending.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TidyPreview {
    pub lines: Vec<String>,
    pub question: String,
    pub not_sent_count: u64,
}

/// The files the user kept ticked in one proposed folder.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidyChoice {
    pub folder_id: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TidyReport {
    pub moved_count: u64,
    pub moved_bytes: u64,
    /// Changed or gone since the proposal, or could not be moved.
    pub skipped_count: u64,
    pub folder_count: u64,
    /// The tidy to undo; `None` when nothing moved.
    pub run_id: Option<String>,
    /// Setup files sent to the quarantine.
    pub quarantine: MoveReport,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UndoReport {
    pub restored_count: u64,
    /// Changed since, moved by the user, or a file of the same name is back where it was.
    pub failed_count: u64,
}

/// A proposed folder before it is checked against the disk.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Proposal {
    name: String,
    dest: Dest,
    why: String,
    /// Indexes into the files sent.
    files: Vec<usize>,
}

/// The last proposal, so that a move takes exactly what was proposed.
#[derive(Debug, Default)]
struct Kept {
    folders: Vec<(String, PathBuf, Vec<ScannedFile>)>,
    installers: Vec<ScannedFile>,
}

#[derive(Default)]
pub struct Tidy {
    plan: Mutex<Kept>,
    /// Held while the undo log is read and written.
    log: Mutex<()>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Moved {
    from: PathBuf,
    to: PathBuf,
    size: u64,
    /// As the file was after the move: undo takes it back only if still so.
    modified: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Run {
    id: String,
    at: u64,
    moves: Vec<Moved>,
    /// Folders this tidy made: removed on undo when empty.
    created: Vec<PathBuf>,
}

impl Run {
    fn info(&self) -> TidyRunInfo {
        let folders: HashSet<&Path> = self.moves.iter().filter_map(|m| m.to.parent()).collect();
        let folder = self
            .moves
            .first()
            .and_then(|m| m.from.parent())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        TidyRunInfo {
            id: self.id.clone(),
            at: self.at,
            folder,
            moved_count: self.moves.len() as u64,
            folder_count: folders.len() as u64,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Log {
    runs: Vec<Run>,
}

/// True when `path` is a file of the last proposal (for previews and "open").
pub fn knows(app: &AppHandle, path: &Path) -> bool {
    app.state::<Tidy>().plan.lock().is_ok_and(|kept| {
        kept.installers.iter().any(|f| f.path == path)
            || kept
                .folders
                .iter()
                .any(|(_, _, files)| files.iter().any(|f| f.path == path))
    })
}

fn has_extension(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| list.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// The files lying loose in `dir` (not in a subfolder) that can be tidied:
/// no partial or very recent downloads, no hidden system files, no links or shortcuts.
fn loose_files(dir: &Path, now: SystemTime) -> Vec<Seen> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Seen> = entries
        .flatten()
        .filter_map(|e| {
            let kind = e.file_type().ok()?;
            if !kind.is_file() || kind.is_symlink() {
                return None;
            }
            let meta = e.metadata().ok()?;
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_lowercase();
            let recent = now
                .duration_since(meta.modified().unwrap_or(UNIX_EPOCH))
                .map_or(true, |age| age < RECENT);
            if files::skip_entry(&meta)
                || has_extension(&path, &PARTIAL)
                || has_extension(&path, &LINKS)
                || recent
                || name == "desktop.ini"
                || name.starts_with("~$")
            {
                return None;
            }
            Some(Seen::from_meta(path, &meta))
        })
        .collect();
    // Newest first: the files the user is most likely to look for.
    out.sort_by_key(|f| std::cmp::Reverse(f.modified));
    out
}

/// Splits off setup files of programs already installed.
fn split_installers(files: Vec<Seen>, installed: &[String]) -> (Vec<Seen>, Vec<Seen>) {
    files
        .into_iter()
        .partition(|f| files::is_used_installer(&f.path, installed))
}

fn size_text(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB * KB {
        format!("{:.1} GB", b / (KB * KB * KB))
    } else if b >= KB * KB {
        format!("{:.0} MB", b / (KB * KB))
    } else {
        format!("{:.0} KB", (b / KB).max(1.0))
    }
}

/// One line per file sent: number, name (the user's name replaced), size, month.
fn lines(files: &[Seen], user: &str) -> Vec<String> {
    files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let name = f
                .path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            format!(
                "{}. {} · {} · {}",
                i + 1,
                prompt::clean_name(&name, user),
                size_text(f.size),
                &iso_date(f.modified)[..7]
            )
        })
        .collect()
}

/// The whole text sent. The folder's own name is not sent: "Downloads", "Desktop" or "a folder".
fn question(lines: &[String], lang: &str, dirs: &Dirs) -> String {
    let language = prompt::language_name(lang);
    let what = match Place::ALL
        .into_iter()
        .find(|p| p.dir().is_some_and(|d| same_dir(&d, &dirs.here)))
    {
        Some(Place::Downloads) => "the Downloads folder",
        Some(Place::Desktop) => "the Desktop",
        _ => "a folder",
    };
    let places = if dirs.libraries() {
        "- place: where the folder goes: \"documents\", \"pictures\" (photos and videos only), \"videos\" (videos only), \"music\" (audio only) or \"here\" (stays inside the folder being tidied, for anything else)."
    } else {
        "- place: always \"here\" (the new folder is made inside the folder being tidied)."
    };
    format!(
        "You help a person who is not technical tidy {what} of their Windows PC. \
Below are the files lying in it: number, name, size and month of the last change. You cannot see what is inside the files.

{}

Group the files that belong together into a few folders with clear, short names (for example \"Bills 2025\", \"Holiday photos\", \"Manuals\").
Reply with one JSON object and nothing else:
{{\"folders\": [{{\"name\": \"...\", \"place\": \"here\", \"why\": \"...\", \"files\": [1, 2]}}]}}

- name: the folder name, two to four words, in {language}. No slashes.
{places}
- why: a few words on what is inside, in {language}.
- files: the numbers of the files that go in it, each file in one folder at most. Leave out files you are not sure about: they stay where they are.
- At most {MAX_FOLDERS} folders. Make a folder for a single file only when it clearly belongs to a theme.",
        lines.join("\n")
    )
}

/// A folder name as it may be created: no path characters, no reserved names, short.
pub fn folder_name(raw: &str) -> Option<String> {
    let s: String = raw
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s: String = s.chars().take(MAX_FOLDER_CHARS).collect();
    let s = s.trim_matches(|c: char| c == '.' || c == ' ').to_string();
    let stem = s
        .split('.')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    (!s.is_empty() && !reserved).then_some(s)
}

/// Reads the AI's folders. `None` when the answer is not the JSON asked for.
/// Files are numbered from 1; each goes into the first folder that names it and
/// fits its kind (no documents in Music), the rest stay where they are.
fn parse_proposals(text: &str, files: &[Seen]) -> Option<Vec<Proposal>> {
    let v = prompt::last_object_with(text, &["folders"])?;
    let list = v.get("folders")?.as_array()?;
    let mut taken = HashSet::new();
    let mut out: Vec<Proposal> = Vec::new();
    for f in list {
        let Some(name) = f.get("name").and_then(|x| x.as_str()).and_then(folder_name) else {
            continue;
        };
        let dest = Dest::from_id(
            &f.get("place")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_ascii_lowercase(),
        );
        let why = prompt::tidy(f.get("why").and_then(|x| x.as_str()).unwrap_or(""), 120);
        let mut picked = Vec::new();
        for n in f
            .get("files")
            .and_then(|x| x.as_array())
            .into_iter()
            .flatten()
        {
            let Some(i) = n.as_u64().and_then(|n| usize::try_from(n).ok()) else {
                continue;
            };
            if i == 0 || i > files.len() || taken.contains(&i) {
                continue;
            }
            if dest.allows(kind_of(&files[i - 1].path)) {
                taken.insert(i);
                picked.push(i - 1);
            }
        }
        if picked.is_empty() {
            continue;
        }
        // The same folder named twice: one folder.
        if let Some(same) = out
            .iter_mut()
            .find(|p| p.dest == dest && p.name.to_lowercase() == name.to_lowercase())
        {
            same.files.extend(picked);
        } else if out.len() < MAX_FOLDERS {
            out.push(Proposal {
                name,
                dest,
                why,
                files: picked,
            });
        } else {
            picked.iter().for_each(|i| {
                taken.remove(&(i + 1));
            });
        }
    }
    Some(out)
}

/// Without the AI: one folder per kind of file, inside the folder being tidied.
fn by_type(files: &[Seen], lang: &str) -> Vec<Proposal> {
    let names: [(FileKind, &str, &str); 7] = [
        (FileKind::Document, "Documents", "Documenti"),
        (FileKind::Image, "Pictures", "Immagini"),
        (FileKind::Video, "Videos", "Video"),
        (FileKind::Audio, "Music", "Musica"),
        (FileKind::Archive, "Archives", "Archivi compressi"),
        (FileKind::Program, "Programs", "Programmi"),
        (FileKind::Disc, "Disk images", "Immagini disco"),
    ];
    names
        .iter()
        .filter_map(|(kind, en, it)| {
            let picked: Vec<usize> = files
                .iter()
                .enumerate()
                .filter(|(_, f)| kind_of(&f.path) == Some(*kind))
                .map(|(i, _)| i)
                .collect();
            (!picked.is_empty()).then(|| Proposal {
                name: (if lang == "it" { *it } else { *en }).to_string(),
                dest: Dest::Here,
                why: String::new(),
                files: picked,
            })
        })
        .collect()
}

fn tidy_file(f: &Seen) -> TidyFile {
    TidyFile {
        path: f.path.to_string_lossy().into_owned(),
        size_bytes: f.size,
        modified: iso_date(f.modified),
    }
}

/// Checks the proposals against the disk and builds what the UI shows and what is kept.
fn plan(
    proposals: Vec<Proposal>,
    sent: &[Seen],
    installers: &[Seen],
    not_sent: u64,
    dirs: &Dirs,
    by_ai: bool,
) -> (TidyPlan, Kept) {
    let mut kept = Kept {
        installers: installers.iter().map(Seen::scanned).collect(),
        ..Kept::default()
    };
    let mut folders = Vec::new();
    let mut placed = 0u64;
    for p in proposals {
        let (dest, base) = dirs.get(p.dest);
        let target = base.join(&p.name);
        // A file with the folder's name is in the way.
        if fs::symlink_metadata(&target).is_ok_and(|m| !m.is_dir()) {
            continue;
        }
        let id = format!("f{}", folders.len() + 1);
        let files: Vec<&Seen> = p.files.iter().map(|i| &sent[*i]).collect();
        placed += files.len() as u64;
        kept.folders.push((
            id.clone(),
            target.clone(),
            files.iter().map(|f| f.scanned()).collect(),
        ));
        folders.push(TidyFolder {
            id,
            name: p.name,
            dest,
            why: p.why,
            exists: target.is_dir(),
            target: target.to_string_lossy().into_owned(),
            size_bytes: files.iter().map(|f| f.size).sum(),
            files: files.into_iter().map(tidy_file).collect(),
        });
    }
    let plan = TidyPlan {
        folders,
        installers: installers.iter().map(tidy_file).collect(),
        left_count: sent.len() as u64 - placed,
        not_sent_count: not_sent,
        by_ai,
    };
    (plan, kept)
}

/// `dir\name`, or `dir\name (2).ext` and so on when that name is taken.
fn free_name(dir: &Path, name: &str) -> Option<PathBuf> {
    let first = dir.join(name);
    if fs::symlink_metadata(&first).is_err() {
        return Some(first);
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..1000)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| fs::symlink_metadata(p).is_err())
}

/// Moves the files the user kept ticked. Returns what moved, for the undo log.
fn apply(kept: &Kept, choices: &[TidyChoice], id: String, at: u64) -> (Run, TidyReport) {
    let mut run = Run {
        id,
        at,
        moves: Vec::new(),
        created: Vec::new(),
    };
    let mut report = TidyReport::default();
    for choice in choices {
        let Some((_, target, files)) = kept.folders.iter().find(|(id, ..)| *id == choice.folder_id)
        else {
            continue;
        };
        let wanted: HashSet<PathBuf> = choice.paths.iter().map(PathBuf::from).collect();
        let mut moved_here = false;
        for f in files.iter().filter(|f| wanted.contains(&f.path)) {
            let moved = (|| {
                if !unchanged_since_scan(&f.path, f.size, f.modified) {
                    return None;
                }
                if !target.is_dir() {
                    fs::create_dir(target).ok()?;
                    run.created.push(target.clone());
                }
                let to = free_name(target, &f.path.file_name()?.to_string_lossy())?;
                move_file(&f.path, &to).ok()?;
                let meta = fs::metadata(&to).ok()?;
                Some(Moved {
                    from: f.path.clone(),
                    to,
                    size: meta.len(),
                    modified: meta.modified().ok()?,
                })
            })();
            match moved {
                Some(m) => {
                    report.moved_count += 1;
                    report.moved_bytes += m.size;
                    moved_here = true;
                    run.moves.push(m);
                }
                None => report.skipped_count += 1,
            }
        }
        if moved_here {
            report.folder_count += 1;
        }
    }
    // A folder made for nothing (every file was skipped) goes away again.
    run.created.retain(|d| fs::remove_dir(d).is_err());
    if !run.moves.is_empty() {
        report.run_id = Some(run.id.clone());
    }
    (run, report)
}

/// Puts the files of `run` back, newest move first, and removes the folders it made when empty.
fn undo(run: &Run) -> UndoReport {
    let mut report = UndoReport::default();
    for m in run.moves.iter().rev() {
        let back = unchanged_since_scan(&m.to, m.size, m.modified)
            && fs::symlink_metadata(&m.from).is_err()
            && move_file(&m.to, &m.from).is_ok();
        if back {
            report.restored_count += 1;
        } else {
            report.failed_count += 1;
        }
    }
    for dir in &run.created {
        let _ = fs::remove_dir(dir);
    }
    report
}

fn log_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("tidy-log.json"))
}

fn read_log(path: &Path) -> Log {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write_log(path: &Path, log: &Log) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_vec(log).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// True when `a` and `b` name the same folder (case and a final separator aside).
fn same_dir(a: &Path, b: &Path) -> bool {
    key(a) == key(b)
}

/// A path as compared: lower case, `/` for separators, no final separator.
fn key(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

/// True when `dir` is `root` or inside it.
fn inside(dir: &Path, root: &Path) -> bool {
    let (d, r) = (key(dir), key(root));
    !r.is_empty() && (d == r || d.starts_with(&format!("{r}/")))
}

/// Folders of Windows and of programs: nothing in them or below them is tidied.
fn system_dirs() -> Vec<PathBuf> {
    [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "ProgramData",
        "APPDATA",
        "LOCALAPPDATA",
    ]
    .iter()
    .filter_map(std::env::var_os)
    .map(PathBuf::from)
    .collect()
}

/// Folders that are refused themselves, though their subfolders may be tidied:
/// the user profile and the drive Windows is on.
fn exact_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = files::profile_dir().into_iter().collect();
    if let Some(drive) = std::env::var_os("SystemDrive") {
        out.push(PathBuf::from(format!("{}\\", drive.to_string_lossy())));
    }
    out
}

/// Why `dir` cannot be tidied, as an i18n key; `None` when it can. Refused: what is
/// not a real folder (missing, a link), Windows and program folders, the user
/// profile and the system drive themselves, the quarantine, and folders that hold
/// a program or a project (loose `.dll` or `.sys` files, a `.git` folder), whose
/// files must stay together.
fn refused(dir: &Path, system: &[PathBuf], exact: &[PathBuf]) -> Option<&'static str> {
    let real = dir.is_absolute() && fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir());
    if !real {
        return Some("tidy.error.notFound");
    }
    let quarantine = dir
        .components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case(DRIVE_FOLDER));
    if quarantine || system.iter().any(|r| inside(dir, r)) || exact.iter().any(|r| same_dir(dir, r))
    {
        return Some("tidy.error.system");
    }
    let program = dir.join(".git").exists()
        || fs::read_dir(dir).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|e| has_extension(&e.path(), &["dll", "sys"]))
        });
    program.then_some("tidy.error.program")
}

/// The folder to tidy and where its files may go. `None` is Downloads. Files from
/// Downloads and the Desktop may also go to Documents, Pictures, Videos and Music;
/// from any other folder, only into new folders inside it.
fn dirs_for(folder: Option<&str>) -> Result<Dirs, String> {
    let Some(folder) = folder else {
        let here = Place::Downloads
            .dir()
            .ok_or_else(|| "tidy.error.noDownloads".to_string())?;
        return Ok(Dirs::with_libraries(here));
    };
    let here = PathBuf::from(folder);
    if let Some(why) = refused(&here, &system_dirs(), &exact_dirs()) {
        return Err(why.to_string());
    }
    let inbox = [Place::Downloads, Place::Desktop]
        .into_iter()
        .any(|p| p.dir().is_some_and(|d| same_dir(&d, &here)));
    Ok(if inbox {
        Dirs::with_libraries(here)
    } else {
        Dirs::only(here)
    })
}

fn user_name() -> String {
    std::env::var("USERNAME").unwrap_or_default()
}

/// The loose files of the folder: the installers apart, the others to group (newest first, at most [`MAX_SENT`]).
fn gather(dirs: &Dirs) -> (Vec<Seen>, Vec<Seen>, u64) {
    let all = loose_files(&dirs.here, SystemTime::now());
    let (installers, mut rest) = split_installers(all, &files::installed_app_names());
    let not_sent = rest.len().saturating_sub(MAX_SENT) as u64;
    rest.truncate(MAX_SENT);
    (rest, installers, not_sent)
}

fn keep_plan(app: &AppHandle, kept: Kept) -> Result<(), String> {
    *app.state::<Tidy>().plan.lock().map_err(|e| e.to_string())? = kept;
    Ok(())
}

/// How many loose files the folder has (Downloads when `folder` is `None`), the
/// folders offered with one click, and the last tidy that can be undone.
#[tauri::command]
pub async fn tidy_overview(app: AppHandle, folder: Option<String>) -> Result<TidyOverview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let last_run = {
            let tidy = app.state::<Tidy>();
            let _lock = tidy.log.lock().map_err(|e| e.to_string())?;
            read_log(&log_path(&app)?).runs.last().map(Run::info)
        };
        let places = [Place::Downloads, Place::Desktop, Place::Documents]
            .into_iter()
            .zip(["downloads", "desktop", "documents"])
            .filter_map(|(p, id)| {
                Some(TidyPlace {
                    id: id.to_string(),
                    path: p.dir()?.to_string_lossy().into_owned(),
                })
            })
            .collect();
        let dirs = match dirs_for(folder.as_deref()) {
            Ok(dirs) => dirs,
            Err(e) if folder.is_none() && e == "tidy.error.noDownloads" => {
                return Ok(TidyOverview {
                    folder: None,
                    places,
                    file_count: 0,
                    size_bytes: 0,
                    last_run,
                });
            }
            Err(e) => return Err(e),
        };
        let files = loose_files(&dirs.here, SystemTime::now());
        Ok(TidyOverview {
            folder: Some(dirs.here.to_string_lossy().into_owned()),
            places,
            file_count: files.len() as u64,
            size_bytes: files.iter().map(|f| f.size).sum(),
            last_run,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens Windows' folder picker over the app; `None` when the user cancels.
#[tauri::command]
pub async fn tidy_pick_folder(
    app: AppHandle,
    title: String,
    start: Option<String>,
) -> Result<Option<String>, String> {
    let owner = crate::elevated::main_window(&app);
    tauri::async_runtime::spawn_blocking(move || {
        platform::pick_folder(owner, &title, start.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What would be sent to the AI.
#[tauri::command]
pub async fn tidy_preview(folder: Option<String>, lang: String) -> Result<TidyPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dirs = dirs_for(folder.as_deref())?;
        let (sent, _, not_sent) = gather(&dirs);
        let lines = lines(&sent, &user_name());
        Ok(TidyPreview {
            question: question(&lines, &lang, &dirs),
            lines,
            not_sent_count: not_sent,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Asks the AI to propose folders. Nothing moves: the proposal is shown first.
#[tauri::command]
pub async fn tidy_ask(
    app: AppHandle,
    folder: Option<String>,
    config: AiConfig,
    lang: String,
) -> Result<TidyPlan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dirs = dirs_for(folder.as_deref())?;
        let (sent, installers, not_sent) = gather(&dirs);
        let proposals = if sent.is_empty() {
            Vec::new()
        } else {
            let text = crate::ai::ask_text(
                &app,
                &config,
                &question(&lines(&sent, &user_name()), &lang, &dirs),
            )?;
            parse_proposals(&text, &sent).ok_or("ai.error.badAnswer")?
        };
        let (plan, kept) = plan(proposals, &sent, &installers, not_sent, &dirs, true);
        keep_plan(&app, kept)?;
        Ok(plan)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Proposes one folder per kind of file, without the AI.
#[tauri::command]
pub async fn tidy_by_type(
    app: AppHandle,
    folder: Option<String>,
    lang: String,
) -> Result<TidyPlan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dirs = dirs_for(folder.as_deref())?;
        let (sent, installers, not_sent) = gather(&dirs);
        let (plan, kept) = plan(
            by_type(&sent, &lang),
            &sent,
            &installers,
            not_sent,
            &dirs,
            false,
        );
        keep_plan(&app, kept)?;
        Ok(plan)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Moves what the user kept ticked, and sends the picked setup files to the
/// quarantine. Only files of the last proposal, unchanged since.
#[tauri::command]
pub async fn tidy_apply(
    app: AppHandle,
    choices: Vec<TidyChoice>,
    installers: Vec<String>,
) -> Result<TidyReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let tidy = app.state::<Tidy>();
        let now = prompt::now_secs();
        let (run, mut report, picked) = {
            let kept = tidy.plan.lock().map_err(|e| e.to_string())?;
            let (run, report) = apply(&kept, &choices, format!("{now}"), now);
            let wanted: HashSet<PathBuf> = installers.iter().map(PathBuf::from).collect();
            let picked: Vec<ScannedFile> = kept
                .installers
                .iter()
                .filter(|f| wanted.contains(&f.path))
                .cloned()
                .collect();
            (run, report, picked)
        };
        if !run.moves.is_empty() {
            let _lock = tidy.log.lock().map_err(|e| e.to_string())?;
            let path = log_path(&app)?;
            let mut log = read_log(&path);
            log.runs.push(run);
            let extra = log.runs.len().saturating_sub(KEEP_RUNS);
            log.runs.drain(..extra);
            write_log(&path, &log)?;
        }
        if !picked.is_empty() {
            cleanup::remove(
                &app.state::<Quarantine>(),
                INSTALLERS_GROUP,
                &picked,
                now,
                &mut report.quarantine,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Undoes a tidy: its files go back where they were and the folders it made go away when empty.
#[tauri::command]
pub async fn tidy_undo(app: AppHandle, run_id: String) -> Result<UndoReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let tidy = app.state::<Tidy>();
        let _lock = tidy.log.lock().map_err(|e| e.to_string())?;
        let path = log_path(&app)?;
        let mut log = read_log(&path);
        let i = log
            .runs
            .iter()
            .position(|r| r.id == run_id)
            .ok_or("tidy.error.noRun")?;
        let run = log.runs.remove(i);
        let report = undo(&run);
        write_log(&path, &log)?;
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(windows)]
mod platform {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
        FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
    };

    /// Windows' own "Select folder" dialog, owned by `owner` so it stays in front of the app.
    pub fn pick_folder(
        owner: Option<isize>,
        title: &str,
        start: Option<&str>,
    ) -> Result<Option<String>, String> {
        // SAFETY: COM is set up for this thread and undone below when we did set it up.
        let init =
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        let result = show(owner, title, start);
        if init.is_ok() {
            // SAFETY: matches the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
        }
        result.map_err(|e| e.message().to_string())
    }

    fn show(
        owner: Option<isize>,
        title: &str,
        start: Option<&str>,
    ) -> windows::core::Result<Option<String>> {
        // SAFETY: plain COM calls on this thread; the returned path is freed below.
        unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(
                dialog.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST,
            )?;
            dialog.SetTitle(&HSTRING::from(title))?;
            if let Some(start) = start {
                if let Ok(item) =
                    SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(start), None)
                {
                    dialog.SetFolder(&item)?;
                }
            }
            if let Err(e) = dialog.Show(owner.map(|h| HWND(h as *mut _))) {
                return if e.code() == ERROR_CANCELLED.to_hresult() {
                    Ok(None)
                } else {
                    Err(e)
                };
            }
            let name = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = name.to_string().ok();
            CoTaskMemFree(Some(name.0 as *const _));
            Ok(path)
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn pick_folder(
        _owner: Option<isize>,
        _title: &str,
        _start: Option<&str>,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{File, FileTimes};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "easy-uninstaller-tidy-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("Downloads")).unwrap();
            fs::create_dir_all(dir.join("Documents")).unwrap();
            fs::create_dir_all(dir.join("Pictures")).unwrap();
            TempDir(dir)
        }

        fn dirs(&self) -> Dirs {
            Dirs {
                here: self.0.join("Downloads"),
                documents: Some(self.0.join("Documents")),
                pictures: Some(self.0.join("Pictures")),
                videos: None,
                music: None,
            }
        }

        fn download(&self, name: &str, size: usize, age_days: u64) -> PathBuf {
            let path = self.0.join("Downloads").join(name);
            fs::write(&path, vec![7u8; size]).unwrap();
            let t = SystemTime::now() - Duration::from_secs(age_days * 86_400 + 3600);
            File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_times(FileTimes::new().set_accessed(t).set_modified(t))
                .unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn file_names(files: &[Seen]) -> Vec<String> {
        files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn only_loose_finished_downloads_are_tidied() {
        let t = TempDir::new("loose");
        t.download("bill.pdf", 10, 30);
        t.download("photo.jpg", 10, 3);
        t.download("movie.mp4.crdownload", 10, 0);
        t.download("desktop.ini", 10, 100);
        t.download("Spotify.lnk", 10, 100);
        fs::write(t.0.join("Downloads/just-now.zip"), b"x").unwrap();
        fs::create_dir_all(t.0.join("Downloads/old folder")).unwrap();
        fs::write(t.0.join("Downloads/old folder/inside.txt"), b"x").unwrap();
        let files = loose_files(&t.0.join("Downloads"), SystemTime::now());
        assert_eq!(
            file_names(&files),
            ["photo.jpg", "bill.pdf"],
            "newest first"
        );
    }

    #[test]
    fn folder_names_cannot_escape() {
        assert_eq!(
            folder_name("Bollette 2025").as_deref(),
            Some("Bollette 2025")
        );
        assert_eq!(folder_name("..\\..\\Windows").as_deref(), Some("Windows"));
        assert_eq!(folder_name("C:/Temp").as_deref(), Some("CTemp"));
        assert_eq!(folder_name(" Foto. ").as_deref(), Some("Foto"));
        assert_eq!(folder_name("con"), None);
        assert_eq!(folder_name("LPT1.txt"), None);
        assert_eq!(folder_name("COMMON").as_deref(), Some("COMMON"));
        assert_eq!(folder_name("..."), None);
        assert_eq!(
            folder_name(&"x".repeat(90)).unwrap().chars().count(),
            MAX_FOLDER_CHARS
        );
    }

    #[test]
    fn the_ai_proposal_is_checked_file_by_file() {
        let t = TempDir::new("parse");
        let sent = vec![
            Seen::from_meta(
                t.download("enel.pdf", 1, 5),
                &fs::metadata(t.0.join("Downloads/enel.pdf")).unwrap(),
            ),
            Seen::from_meta(
                t.download("beach.jpg", 1, 6),
                &fs::metadata(t.0.join("Downloads/beach.jpg")).unwrap(),
            ),
            Seen::from_meta(
                t.download("song.mp3", 1, 7),
                &fs::metadata(t.0.join("Downloads/song.mp3")).unwrap(),
            ),
        ];
        let text = r#"Sure. {"folders": [
            {"name": "Bollette", "place": "documents", "why": "Bollette\nluce", "files": [1, 1, 9, 0, "2"]},
            {"name": "Foto mare", "place": "pictures", "files": [2, 3, 1]},
            {"name": "bollette", "place": "documents", "files": [3]},
            {"name": "../..", "place": "documents", "files": [3]},
            {"name": "Vuota", "place": "music", "files": []}
        ]}"#;
        let p = parse_proposals(text, &sent).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].name, "Bollette");
        assert_eq!(p[0].dest, Dest::Documents);
        assert_eq!(p[0].why, "Bollette luce");
        assert_eq!(p[0].files, [0, 2], "the same name twice is one folder");
        assert_eq!(
            p[1].files,
            [1],
            "a song does not go to Pictures, a file goes in one folder"
        );
        assert_eq!(parse_proposals("I would sort by date.", &sent), None);
    }

    #[test]
    fn tidy_moves_and_undo_puts_back() {
        let t = TempDir::new("apply");
        let dirs = t.dirs();
        t.download("enel.pdf", 100, 5);
        t.download("beach.jpg", 200, 6);
        t.download("clip.mp4", 300, 7);
        t.download("ChromeSetup.exe", 50, 9);
        // Already in Documents\Bollette: the new one gets another name.
        fs::create_dir_all(t.0.join("Documents/Bollette")).unwrap();
        fs::write(t.0.join("Documents/Bollette/enel.pdf"), b"old").unwrap();

        let all = loose_files(&dirs.here, SystemTime::now());
        let (installers, sent) = split_installers(all, &["Google Chrome".to_string()]);
        assert_eq!(file_names(&installers), ["ChromeSetup.exe"]);
        let names = file_names(&sent);
        let at = |n: &str| names.iter().position(|x| x == n).unwrap() + 1;
        let text = format!(
            r#"{{"folders": [{{"name": "Bollette", "place": "documents", "files": [{}]}}, {{"name": "Video", "place": "videos", "files": [{}]}}, {{"name": "Foto", "place": "pictures", "files": [{}]}}]}}"#,
            at("enel.pdf"),
            at("clip.mp4"),
            at("beach.jpg")
        );
        let proposals = parse_proposals(&text, &sent).unwrap();
        let (plan, kept) = plan(proposals, &sent, &installers, 0, &dirs, true);
        assert_eq!(plan.folders.len(), 3);
        assert!(plan.folders[0].exists);
        assert_eq!(
            plan.folders[1].dest,
            Dest::Here,
            "no Videos folder: it stays in Downloads"
        );
        assert_eq!(plan.left_count, 0);

        // The photo is unticked; the video changes after the proposal.
        fs::write(t.0.join("Downloads/clip.mp4"), vec![1u8; 301]).unwrap();
        let choices = vec![
            TidyChoice {
                folder_id: "f1".into(),
                paths: vec![plan.folders[0].files[0].path.clone()],
            },
            TidyChoice {
                folder_id: "f2".into(),
                paths: vec![plan.folders[1].files[0].path.clone()],
            },
            TidyChoice {
                folder_id: "f9".into(),
                paths: vec![t
                    .0
                    .join("Downloads/beach.jpg")
                    .to_string_lossy()
                    .into_owned()],
            },
        ];
        let (run, report) = apply(&kept, &choices, "1".into(), 1);
        assert_eq!(
            (
                report.moved_count,
                report.skipped_count,
                report.folder_count
            ),
            (1, 1, 1)
        );
        assert!(t.0.join("Documents/Bollette/enel (2).pdf").is_file());
        assert_eq!(
            fs::read(t.0.join("Documents/Bollette/enel.pdf")).unwrap(),
            b"old"
        );
        assert!(
            !t.0.join("Downloads/Video").exists(),
            "a folder made for nothing is removed"
        );
        assert!(t.0.join("Downloads/beach.jpg").is_file());
        assert!(t.0.join("Downloads/clip.mp4").is_file());

        let back = undo(&run);
        assert_eq!((back.restored_count, back.failed_count), (1, 0));
        assert!(t.0.join("Downloads/enel.pdf").is_file());
        assert!(
            t.0.join("Documents/Bollette/enel.pdf").is_file(),
            "the folder was there before: it stays"
        );
    }

    #[test]
    fn undo_leaves_changed_files_alone_and_removes_new_empty_folders() {
        let t = TempDir::new("undo");
        let dirs = t.dirs();
        t.download("a.pdf", 10, 5);
        t.download("b.pdf", 10, 5);
        let sent = loose_files(&dirs.here, SystemTime::now());
        let (plan, kept) = plan(by_type(&sent, "it"), &sent, &[], 0, &dirs, false);
        assert_eq!(plan.folders[0].name, "Documenti");
        assert!(!plan.folders[0].exists);
        let paths = plan.folders[0]
            .files
            .iter()
            .map(|f| f.path.clone())
            .collect();
        let (run, report) = apply(
            &kept,
            &[TidyChoice {
                folder_id: "f1".into(),
                paths,
            }],
            "2".into(),
            2,
        );
        assert_eq!(report.moved_count, 2);
        assert_eq!(run.created, [t.0.join("Downloads/Documenti")]);
        fs::write(t.0.join("Downloads/Documenti/a.pdf"), b"edited after").unwrap();
        let back = undo(&run);
        assert_eq!((back.restored_count, back.failed_count), (1, 1));
        assert!(t.0.join("Downloads/b.pdf").is_file());
        assert!(
            t.0.join("Downloads/Documenti/a.pdf").is_file(),
            "an edited file is left where it is"
        );
    }

    #[test]
    fn only_names_sizes_and_months_are_sent() {
        let t = TempDir::new("sent");
        t.download("CV Mario Rossi.pdf", 2048, 40);
        let sent = loose_files(&t.0.join("Downloads"), SystemTime::now());
        let l = lines(&sent, "mario");
        assert_eq!(l.len(), 1);
        assert!(l[0].starts_with("1. CV %USERNAME% Rossi.pdf · 2 KB · "));
        let q = question(&l, "it", &Dirs::only(t.0.join("Downloads")));
        assert!(q.contains("in Italian"));
        assert!(!q.contains(&t.0.to_string_lossy().into_owned()), "no paths");
        assert!(q.contains("always \"here\""), "no other places offered");
        assert!(!q.contains("\"documents\""));
        let q = question(&l, "it", &t.dirs());
        assert!(q.contains("\"documents\", \"pictures\""));
    }

    #[test]
    fn a_picked_folder_gets_folders_inside_it_only() {
        let t = TempDir::new("picked");
        let dirs = Dirs::only(t.0.join("Downloads"));
        t.download("enel.pdf", 10, 5);
        let sent = loose_files(&dirs.here, SystemTime::now());
        let text = r#"{"folders": [{"name": "Bollette", "place": "documents", "files": [1]}]}"#;
        let (plan, _) = plan(
            parse_proposals(text, &sent).unwrap(),
            &sent,
            &[],
            0,
            &dirs,
            true,
        );
        assert_eq!(plan.folders[0].dest, Dest::Here);
        assert_eq!(
            PathBuf::from(&plan.folders[0].target),
            t.0.join("Downloads/Bollette")
        );
    }

    #[test]
    fn system_program_and_project_folders_are_refused() {
        let t = TempDir::new("refused");
        let system = [t.0.join("Windows"), t.0.join("Program Files")];
        let exact = [t.0.join("Users/me")];
        for d in [
            "Windows/System32",
            "Program Files/App",
            "Users/me/Foto",
            "Users/me/Documents",
            "Game",
            "Code/.git",
            "Easy Uninstaller Quarantine/1",
        ] {
            fs::create_dir_all(t.0.join(d)).unwrap();
        }
        fs::write(t.0.join("Game/engine.dll"), b"x").unwrap();
        let check = |rel: &str| refused(&t.0.join(rel), &system, &exact);
        assert_eq!(check("Windows"), Some("tidy.error.system"));
        assert_eq!(check("Windows/System32"), Some("tidy.error.system"));
        assert_eq!(check("Program Files/App"), Some("tidy.error.system"));
        assert_eq!(check("Users/me"), Some("tidy.error.system"));
        assert_eq!(
            check("Easy Uninstaller Quarantine/1"),
            Some("tidy.error.system")
        );
        assert_eq!(check("Game"), Some("tidy.error.program"));
        assert_eq!(check("Code"), Some("tidy.error.program"));
        assert_eq!(check("Missing"), Some("tidy.error.notFound"));
        assert_eq!(
            refused(Path::new("Foto"), &system, &exact),
            Some("tidy.error.notFound")
        );
        assert_eq!(check("Users/me/Foto"), None);
        assert_eq!(check("Users/me/Documents"), None);
        assert_eq!(check("Downloads"), None);
        assert!(!inside(&t.0.join("Windows2"), &t.0.join("Windows")));
        assert!(same_dir(
            Path::new("C:\\Users\\Me\\"),
            Path::new("c:/users/me")
        ));
    }
}
