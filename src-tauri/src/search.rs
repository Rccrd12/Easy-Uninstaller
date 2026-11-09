//! Finding files by writing a sentence (AI help, phase 2).
//!
//! The AI only turns the sentence into a [`SearchFilter`]: kinds of file, sizes,
//! dates, folders and a few words. The search itself runs here, on this PC, so
//! the names of the user's files never leave it. Only the sentence is sent.
//!
//! The filter is shown to the user as plain chips they can change, and the
//! search is read only. Files the user picks go to the quarantine through
//! [`cleanup::remove`], and only files this search found, unchanged since.

use crate::ai::prompt;
use crate::ai::AiConfig;
use crate::cleanup;
use crate::files::{self, iso_date, ScannedFile, Seen};
use crate::quarantine::{MoveReport, Quarantine};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Manager};

/// How many results go to the UI (the first ones in the chosen order).
const MAX_SHOWN: usize = 200;
/// How many results are kept for a move. More than this and the user is asked to narrow it down.
const MAX_KEPT: usize = 5000;
/// The walk stops after this many files, so a search never takes minutes.
const MAX_WALK: u64 = 400_000;
const MAX_WORDS: usize = 6;
const MAX_WORD_CHARS: usize = 40;
/// A sentence longer than this is refused (it is not a search).
pub const MAX_SENTENCE_CHARS: usize = 300;
const MB: u64 = 1024 * 1024;
const DAY_SECS: u64 = 24 * 60 * 60;
/// Group id of what this search moves: the quarantine entry is titled with it.
pub const GROUP_ID: &str = "found";

/// What a file is, by its extension. Mirrors `KINDS` in `src/lib/paths.ts`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Image,
    Video,
    Audio,
    Document,
    Archive,
    Program,
    Disc,
}

const KINDS: [(FileKind, &[&str]); 7] = [
    (
        FileKind::Image,
        &[
            "jpg", "jpeg", "png", "gif", "bmp", "webp", "heic", "heif", "tif", "tiff", "dng",
            "cr2", "nef", "arw", "psd", "svg",
        ],
    ),
    (
        FileKind::Video,
        &[
            "mp4", "mov", "avi", "mkv", "wmv", "m4v", "webm", "3gp", "mts",
        ],
    ),
    (
        FileKind::Audio,
        &["mp3", "wav", "flac", "m4a", "aac", "ogg", "wma"],
    ),
    (
        FileKind::Document,
        &[
            "pdf", "txt", "md", "rtf", "csv", "log", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
            "odt", "ods", "odp", "epub",
        ],
    ),
    (FileKind::Archive, &["zip", "7z", "rar", "gz", "tar"]),
    (
        FileKind::Program,
        &["exe", "msi", "msix", "msixbundle", "appx", "bat", "cmd"],
    ),
    (
        FileKind::Disc,
        &["iso", "img", "vdi", "vhd", "vhdx", "vmdk"],
    ),
];

impl FileKind {
    const ALL: [FileKind; 7] = [
        FileKind::Image,
        FileKind::Video,
        FileKind::Audio,
        FileKind::Document,
        FileKind::Archive,
        FileKind::Program,
        FileKind::Disc,
    ];

    fn id(self) -> &'static str {
        match self {
            FileKind::Image => "image",
            FileKind::Video => "video",
            FileKind::Audio => "audio",
            FileKind::Document => "document",
            FileKind::Archive => "archive",
            FileKind::Program => "program",
            FileKind::Disc => "disc",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }
}

/// The kind of a file by its extension; `None` for anything else.
pub fn kind_of(path: &Path) -> Option<FileKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    KINDS
        .iter()
        .find(|(_, exts)| exts.contains(&ext.as_str()))
        .map(|(k, _)| *k)
}

/// A folder of the user profile, as Windows names it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    Desktop,
    Documents,
    Downloads,
    Pictures,
    Videos,
    Music,
}

impl Place {
    pub const ALL: [Place; 6] = [
        Place::Desktop,
        Place::Documents,
        Place::Downloads,
        Place::Pictures,
        Place::Videos,
        Place::Music,
    ];

    fn id(self) -> &'static str {
        match self {
            Place::Desktop => "desktop",
            Place::Documents => "documents",
            Place::Downloads => "downloads",
            Place::Pictures => "pictures",
            Place::Videos => "videos",
            Place::Music => "music",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Where it is on this PC, also when the user moved it (to OneDrive, another drive).
    pub fn dir(self) -> Option<PathBuf> {
        let (value, fallback) = match self {
            Place::Desktop => ("Desktop", "Desktop"),
            Place::Documents => ("Personal", "Documents"),
            Place::Downloads => ("{374DE290-123F-4565-9164-39C4925E467B}", "Downloads"),
            Place::Pictures => ("My Pictures", "Pictures"),
            Place::Videos => ("My Video", "Videos"),
            Place::Music => ("My Music", "Music"),
        };
        files::shell_folder(value, fallback)
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortBy {
    /// Largest first.
    #[default]
    Size,
    /// Least recently used first.
    Oldest,
    /// Most recently changed first.
    Newest,
}

/// What to look for. Every part is optional; an empty filter lists every file.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilter {
    /// Any of these kinds; empty for any file.
    #[serde(default)]
    pub kinds: Vec<FileKind>,
    #[serde(default)]
    pub min_bytes: Option<u64>,
    #[serde(default)]
    pub max_bytes: Option<u64>,
    /// Not opened or changed for at least this many days.
    #[serde(default)]
    pub unused_days: Option<u32>,
    /// Changed in the last this many days.
    #[serde(default)]
    pub recent_days: Option<u32>,
    /// Only in these folders; empty for the whole user profile.
    #[serde(default)]
    pub places: Vec<Place>,
    /// Words that must all be in the path (file or folder names), any case.
    #[serde(default)]
    pub words: Vec<String>,
    #[serde(default)]
    pub sort: SortBy,
    /// Parts of the sentence a file name cannot show ("of the holidays"), as written.
    /// Never searched for: only shown, so the person knows the results ignore them.
    #[serde(default)]
    pub unsure: Vec<String>,
}

impl SearchFilter {
    /// The same filter within sane bounds: no repeats, sizes in order, words short and few.
    pub fn normalized(mut self) -> Self {
        let mut seen = HashSet::new();
        self.kinds.retain(|k| seen.insert(*k));
        let mut seen = HashSet::new();
        self.places.retain(|p| seen.insert(*p));
        if self.places.len() == Place::ALL.len() {
            self.places.clear();
        }
        self.min_bytes = self.min_bytes.filter(|b| *b > 0);
        self.max_bytes = self.max_bytes.filter(|b| *b > 0);
        if let (Some(min), Some(max)) = (self.min_bytes, self.max_bytes) {
            if min > max {
                self.min_bytes = Some(max);
                self.max_bytes = Some(min);
            }
        }
        let days = |d: Option<u32>| d.filter(|d| *d > 0).map(|d| d.min(36_500));
        self.unused_days = days(self.unused_days);
        self.recent_days = days(self.recent_days);
        let mut words: Vec<String> = Vec::new();
        for w in &self.words {
            let w = clean_word(w);
            if !w.is_empty() && !words.iter().any(|x| x.eq_ignore_ascii_case(&w)) {
                words.push(w);
            }
        }
        words.truncate(MAX_WORDS);
        self.words = words;
        let mut unsure: Vec<String> = Vec::new();
        for u in &self.unsure {
            let u: String = u.chars().filter(|c| !c.is_control()).take(60).collect();
            let u = u.trim().to_string();
            if !u.is_empty() && !unsure.iter().any(|x| x.eq_ignore_ascii_case(&u)) {
                unsure.push(u);
            }
        }
        unsure.truncate(3);
        self.unsure = unsure;
        self
    }

    fn matches(&self, rel: &str, f: &Seen, now: SystemTime) -> bool {
        let age = |t: SystemTime| now.duration_since(t).unwrap_or(Duration::ZERO);
        if !self.kinds.is_empty() && !kind_of(&f.path).is_some_and(|k| self.kinds.contains(&k)) {
            return false;
        }
        if self.min_bytes.is_some_and(|m| f.size < m) || self.max_bytes.is_some_and(|m| f.size > m)
        {
            return false;
        }
        if self
            .unused_days
            .is_some_and(|d| age(f.last_used) < Duration::from_secs(u64::from(d) * DAY_SECS))
        {
            return false;
        }
        if self
            .recent_days
            .is_some_and(|d| age(f.modified) > Duration::from_secs(u64::from(d) * DAY_SECS))
        {
            return false;
        }
        let rel = rel.to_lowercase();
        self.words.iter().all(|w| rel.contains(&w.to_lowercase()))
    }
}

/// A word as searched: one line, no path separators or wildcards, short.
fn clean_word(word: &str) -> String {
    let w: String = word
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '\\' | '/' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    w.trim().chars().take(MAX_WORD_CHARS).collect()
}

/// A file found by the search, as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FoundFile {
    pub path: String,
    pub size_bytes: u64,
    /// The later of last opened and last changed, as `YYYY-MM-DD`.
    pub last_used: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// The filter as used (after [`SearchFilter::normalized`]).
    pub filter: SearchFilter,
    /// The first [`MAX_SHOWN`] in the chosen order.
    pub files: Vec<FoundFile>,
    pub total_count: u64,
    pub total_bytes: u64,
    /// More than [`MAX_KEPT`] matches: only the first ones can be moved.
    pub too_many: bool,
    /// The walk stopped early (a very large profile).
    pub partial: bool,
}

/// The files of the last search, so that a move takes exactly what was found.
#[derive(Default)]
pub struct LastSearch(Mutex<Vec<ScannedFile>>);

/// True when `path` is a file the last search found (for previews and "open").
pub fn knows(app: &AppHandle, path: &Path) -> bool {
    app.state::<LastSearch>()
        .0
        .lock()
        .is_ok_and(|kept| kept.iter().any(|f| f.path == path))
}

/// The folders searched for `places`: the whole profile when empty, plus folders
/// the user moved out of it. Nested folders are searched once.
fn roots(places: &[Place]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if places.is_empty() {
        dirs.extend(files::profile_dir());
        dirs.extend(Place::ALL.iter().filter_map(|p| p.dir()));
    } else {
        dirs.extend(places.iter().filter_map(|p| p.dir()));
    }
    files::dedup_dirs(&dirs)
}

/// Looks through `roots` for files matching `filter`, in the filter's order.
/// Returns the matches and whether the walk stopped early.
pub fn search(roots: &[PathBuf], filter: &SearchFilter, now: SystemTime) -> (Vec<Seen>, bool) {
    let mut found = Vec::new();
    let mut walked = 0u64;
    for root in roots {
        files::walk(root, &files::skip_personal_dir, &mut |path, meta| {
            walked += 1;
            if walked > MAX_WALK {
                return;
            }
            let seen = Seen::from_meta(path, meta);
            let rel = seen
                .path
                .strip_prefix(root)
                .unwrap_or(&seen.path)
                .to_string_lossy()
                .into_owned();
            if filter.matches(&rel, &seen, now) {
                found.push(seen);
            }
        });
    }
    match filter.sort {
        SortBy::Size => found.sort_by_key(|f| std::cmp::Reverse(f.size)),
        SortBy::Oldest => found.sort_by_key(|f| f.last_used),
        SortBy::Newest => found.sort_by_key(|f| std::cmp::Reverse(f.modified)),
    }
    (found, walked > MAX_WALK)
}

fn result(filter: SearchFilter, found: &[Seen], partial: bool) -> SearchResult {
    SearchResult {
        files: found
            .iter()
            .take(MAX_SHOWN)
            .map(|f| FoundFile {
                path: f.path.to_string_lossy().into_owned(),
                size_bytes: f.size,
                last_used: iso_date(f.last_used),
            })
            .collect(),
        total_count: found.len() as u64,
        total_bytes: found.iter().map(|f| f.size).sum(),
        too_many: found.len() > MAX_KEPT,
        partial,
        filter,
    }
}

/// Runs the search on this PC (read only) and keeps its files for a later move.
#[tauri::command]
pub async fn search_files(app: AppHandle, filter: SearchFilter) -> Result<SearchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let filter = filter.normalized();
        let (found, partial) = search(&roots(&filter.places), &filter, SystemTime::now());
        let kept: Vec<ScannedFile> = found.iter().take(MAX_KEPT).map(Seen::scanned).collect();
        *app.state::<LastSearch>()
            .0
            .lock()
            .map_err(|e| e.to_string())? = kept;
        Ok(result(filter, &found, partial))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Moves the picked files of the last search to the quarantine (one entry).
/// Only paths the search found are taken, and only if unchanged since.
#[tauri::command]
pub async fn quarantine_found(app: AppHandle, paths: Vec<String>) -> Result<MoveReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let wanted: HashSet<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let picked: Vec<ScannedFile> = app
            .state::<LastSearch>()
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .filter(|f| wanted.contains(&f.path))
            .cloned()
            .collect();
        let mut report = MoveReport::default();
        if !picked.is_empty() {
            cleanup::remove(
                &app.state::<Quarantine>(),
                GROUP_ID,
                &picked,
                prompt::now_secs(),
                &mut report,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── The sentence, turned into a filter by the AI ───────────────────────────

/// The text sent: the user's sentence, today's date and the fields to fill. Nothing about the PC.
pub fn question(sentence: &str, lang: &str, today: &str) -> String {
    let language = prompt::language_name(lang);
    format!(
        "You turn a request for finding files on a Windows PC into a search filter. \
You do not see any file: the search runs on the PC with your filter, and the person checks the results.
Today is {today}. The request is written in {language}.

Request: \"{sentence}\"

Reply with one JSON object and nothing else:
{{\"kinds\": [], \"minSizeMb\": null, \"maxSizeMb\": null, \"unusedDays\": null, \"recentDays\": null, \"places\": [], \"words\": [], \"unsure\": [], \"sort\": \"size\"}}

- kinds: any of \"image\", \"video\", \"audio\", \"document\", \"archive\", \"program\", \"disc\" (disk images like .iso); [] for any file.
- minSizeMb, maxSizeMb: sizes in MB (1 GB = 1024 MB), or null.
- unusedDays: not opened or changed for at least this many days (\"not opened for a year\" = 365), or null.
- recentDays: changed in the last this many days (\"this week\" = 7), or null.
- places: any of \"desktop\", \"documents\", \"downloads\", \"pictures\", \"videos\", \"music\"; [] for all the person's folders.
- words: only words the person clearly wants in the file or folder name (a name, a brand, \"invoice\"), as they would be written in a file name; [] for topics a name may not show.
- unsure: the parts of the request that a file's name, type, size, date or folder cannot tell (a topic, an event, who or what is in it, like \"of the holidays\"), copied as written in the request; [] when there are none.
- sort: \"size\" (largest first), \"oldest\" (unused the longest) or \"newest\".

If the request is not about finding files, reply {{\"notAbout\": true}}."
    )
}

/// Reads the AI's filter. `None` when it is not the JSON asked for, or says the
/// sentence is not a search. Unknown values are dropped, numbers bounded.
pub fn parse_filter(text: &str) -> Option<SearchFilter> {
    let v = prompt::last_object_with(text, &["kinds", "places", "sort", "notAbout"])?;
    if v.get("notAbout").and_then(|x| x.as_bool()) == Some(true) {
        return None;
    }
    let strings = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| s.trim().to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default()
    };
    let number = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_f64())
            .filter(|n| n.is_finite() && *n > 0.0)
    };
    let mb = |k: &str| number(k).map(|n| (n.min(16.0 * 1024.0 * 1024.0) * MB as f64) as u64);
    let days = |k: &str| number(k).map(|n| n.round().min(36_500.0) as u32);
    let sort = match v
        .get("sort")
        .and_then(|x| x.as_str())
        .map(str::to_ascii_lowercase)
    {
        Some(s) if s == "oldest" => SortBy::Oldest,
        Some(s) if s == "newest" => SortBy::Newest,
        _ => SortBy::Size,
    };
    let words = v
        .get("words")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some(
        SearchFilter {
            kinds: strings("kinds")
                .iter()
                .filter_map(|k| FileKind::from_id(k))
                .collect(),
            min_bytes: mb("minSizeMb"),
            max_bytes: mb("maxSizeMb"),
            unused_days: days("unusedDays"),
            recent_days: days("recentDays"),
            places: strings("places")
                .iter()
                .filter_map(|p| Place::from_id(p))
                .collect(),
            words,
            sort,
            unsure: v
                .get("unsure")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        }
        .normalized(),
    )
}

/// The AI said the sentence is not a search.
fn not_a_search(text: &str) -> bool {
    prompt::last_object_with(text, &["notAbout"])
        .is_some_and(|v| v.get("notAbout").and_then(|x| x.as_bool()) == Some(true))
}

/// `YYYY-MM-DD` of today, for the AI to count days from.
fn today() -> String {
    iso_date(SystemTime::now())
}

/// Asks the AI to turn `sentence` into a filter. Only the sentence is sent.
#[tauri::command]
pub async fn search_understand(
    app: AppHandle,
    config: AiConfig,
    sentence: String,
    lang: String,
) -> Result<SearchFilter, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let sentence = prompt::tidy(&sentence, MAX_SENTENCE_CHARS);
        if sentence.is_empty() {
            return Err("search.error.empty".to_string());
        }
        let text = crate::ai::ask_text(&app, &config, &question(&sentence, &lang, &today()))?;
        if not_a_search(&text) {
            return Err("search.error.notSearch".into());
        }
        parse_filter(&text).ok_or_else(|| "ai.error.badAnswer".into())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File, FileTimes};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "easy-uninstaller-search-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn file(&self, rel: &str, size: usize, age_days: u64) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, vec![1u8; size]).unwrap();
            let t = SystemTime::now() - Duration::from_secs(age_days * DAY_SECS);
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

    fn names(found: &[Seen]) -> Vec<String> {
        found
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_filter_runs_on_this_pc() {
        let t = TempDir::new("run");
        t.file("Pictures/Holidays 2022/drone.mp4", 5000, 400);
        t.file("Downloads/GOPR0193.MP4", 3000, 500);
        t.file("Downloads/clip.mp4", 100, 500); // too small
        t.file("Desktop/new.mov", 4000, 2); // used recently
        t.file("Documents/invoice.pdf", 9000, 800); // not a video
        t.file("AppData/Local/cache.mp4", 9000, 800); // app data is never searched
        t.file(".hidden/x.mp4", 9000, 800);
        let filter = SearchFilter {
            kinds: vec![FileKind::Video],
            min_bytes: Some(1000),
            unused_days: Some(365),
            ..SearchFilter::default()
        };
        let (found, partial) = search(std::slice::from_ref(&t.0), &filter, SystemTime::now());
        assert!(!partial);
        assert_eq!(
            names(&found),
            ["drone.mp4", "GOPR0193.MP4"],
            "largest first"
        );

        let words = SearchFilter {
            words: vec!["holidays".into()],
            ..SearchFilter::default()
        };
        let (found, _) = search(std::slice::from_ref(&t.0), &words, SystemTime::now());
        assert_eq!(names(&found), ["drone.mp4"], "words match folder names too");

        let recent = SearchFilter {
            recent_days: Some(7),
            sort: SortBy::Newest,
            ..SearchFilter::default()
        };
        let (found, _) = search(std::slice::from_ref(&t.0), &recent, SystemTime::now());
        assert_eq!(names(&found), ["new.mov"]);

        let oldest = SearchFilter {
            kinds: vec![FileKind::Video, FileKind::Document],
            sort: SortBy::Oldest,
            ..SearchFilter::default()
        };
        let (found, _) = search(std::slice::from_ref(&t.0), &oldest, SystemTime::now());
        assert_eq!(names(&found)[0], "invoice.pdf");
    }

    #[test]
    fn kinds_follow_the_extension() {
        assert_eq!(kind_of(Path::new("a/B.JPG")), Some(FileKind::Image));
        assert_eq!(kind_of(Path::new("setup.msi")), Some(FileKind::Program));
        assert_eq!(kind_of(Path::new("win.iso")), Some(FileKind::Disc));
        assert_eq!(kind_of(Path::new("notes")), None);
        assert_eq!(kind_of(Path::new("data.bin")), None);
    }

    #[test]
    fn the_ai_answer_becomes_a_bounded_filter() {
        let text = "Here you go: {\"kinds\": [\"video\", \"spreadsheet\", \"VIDEO\"], \"minSizeMb\": 500, \"maxSizeMb\": -3, \"unusedDays\": 365.4, \"recentDays\": null, \"places\": [\"pictures\", \"downloads\", \"system32\"], \"words\": [\"vacanze\", \"..\\\\Windows\", \"\", \"Vacanze\"], \"sort\": \"oldest\", \"delete\": true}";
        let f = parse_filter(text).unwrap();
        assert_eq!(f.kinds, [FileKind::Video]);
        assert_eq!(f.min_bytes, Some(500 * MB));
        assert_eq!(f.max_bytes, None);
        assert_eq!(f.unused_days, Some(365));
        assert_eq!(f.recent_days, None);
        assert_eq!(f.places, [Place::Pictures, Place::Downloads]);
        assert_eq!(f.words, ["vacanze", "..Windows"]);
        assert_eq!(f.sort, SortBy::Oldest);
        assert!(f.unsure.is_empty(), "no field, nothing unsure");

        let holidays = parse_filter("{\"kinds\": [\"video\"], \"words\": [], \"unsure\": [\"delle vacanze\", \"\", \"Delle vacanze\", \"a\\nb\"], \"sort\": \"size\"}").unwrap();
        assert_eq!(holidays.unsure, ["delle vacanze", "ab"]);
        assert!(
            holidays.words.is_empty(),
            "a topic is never searched in the names"
        );

        assert_eq!(parse_filter("{\"notAbout\": true}"), None);
        assert!(not_a_search("{\"notAbout\": true}"));
        assert!(!not_a_search("{\"kinds\": [], \"notAbout\": false}"));
        assert_eq!(parse_filter("I would look in Pictures."), None);
        let all = parse_filter("{\"kinds\": [], \"places\": [\"desktop\", \"documents\", \"downloads\", \"pictures\", \"videos\", \"music\"]}").unwrap();
        assert!(all.places.is_empty(), "every folder is the same as none");
    }

    #[test]
    fn sizes_and_words_are_kept_sane() {
        let f = SearchFilter {
            min_bytes: Some(10 * MB),
            max_bytes: Some(MB),
            words: (0..10).map(|i| format!("w{i}")).collect(),
            unused_days: Some(0),
            ..SearchFilter::default()
        }
        .normalized();
        assert_eq!((f.min_bytes, f.max_bytes), (Some(MB), Some(10 * MB)));
        assert_eq!(f.words.len(), MAX_WORDS);
        assert_eq!(f.unused_days, None);
    }

    #[test]
    fn only_the_sentence_is_sent() {
        let q = question("video delle vacanze", "it", "2026-11-09");
        assert!(q.contains("Request: \"video delle vacanze\""));
        assert!(q.contains("Today is 2026-11-09"));
        assert!(q.contains("written in Italian"));
        assert!(!q.contains("\\Users\\"));
    }
}
