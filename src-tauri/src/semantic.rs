//! The local index: finding photos and videos by what they show ("holidays at the
//! sea") when their names say nothing (W20251201RM.mp4), and documents by what
//! they say.
//!
//! - Photos and videos: EmbeddingGemma 2 runs on this PC through llama.cpp's
//!   server (`engine.rs`), downloaded on request and checked by SHA-256
//!   (`models.rs`). It turns each picture into a vector, kept in `index.bin`
//!   (`store.rs`); a search turns the words into a vector and the closest
//!   pictures win.
//! - Documents: Windows Search already reads them (`docs.rs`).
//!
//! Read only: the index looks at files, it never changes, moves or removes one.
//! Everything it keeps is in its own folder, which "Remove" deletes whole.

mod docs;
mod engine;
mod frames;
mod models;
mod sha256;
mod store;

use crate::ai::http::{self, HttpError, Url};
use crate::files;
use crate::search::{kind_of, FileKind};
use models::{Pinned, Quality, ENGINE, ENGINE_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use store::Index;
use tauri::{AppHandle, Emitter, Manager};

/// The UI hears every change of [`IndexStatus`] through this event.
pub const EVENT: &str = "local-index";
/// Photos smaller than this are icons and stickers, not photos.
const MIN_PHOTO_BYTES: u64 = 30 * 1024;
/// The walk for the index stops after this many files.
const MAX_WALK: u64 = 3_000_000;
/// The index is written at least this often while it grows.
const SAVE_EVERY: Duration = Duration::from_secs(60);
/// The index starts this long after the app, so opening it stays quick.
const START_DELAY: Duration = Duration::from_secs(20);

/// How many frames of a video are looked at. Mirrors `IndexDepth` in types.ts.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Depth {
    /// Only the preview Windows shows.
    Quick,
    /// A few frames spread over the video.
    #[default]
    Partial,
    /// One frame every few seconds: the whole video.
    Full,
}

impl Depth {
    fn code(self) -> u8 {
        match self {
            Depth::Quick => 1,
            Depth::Partial => 2,
            Depth::Full => 3,
        }
    }
}

/// Which model looks at the pictures. Mirrors `IndexSource` in types.ts.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// EmbeddingGemma 2, downloaded by the app (`quality` says which).
    #[default]
    Download,
    /// GGUF files already on this PC, run by the downloaded llama.cpp.
    Files,
    /// A llama.cpp server the person runs on this PC.
    Server,
}

/// The choices of Settings > Local index. Mirrors `IndexSettings` in types.ts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct IndexSettings {
    pub source: Source,
    pub quality: Quality,
    /// `Files`: the model (`.gguf`) and the part that reads pictures (`mmproj…gguf`).
    pub own_model: String,
    pub own_vision: String,
    /// `Server`: its address, on this PC only.
    pub server_url: String,
    pub depth: Depth,
    pub photos: bool,
    pub videos: bool,
    pub documents: bool,
    /// Every fixed drive, not only the user's folders.
    pub whole_pc: bool,
    pub paused: bool,
}

impl Default for IndexSettings {
    fn default() -> Self {
        IndexSettings {
            source: Source::Download,
            quality: Quality::Light,
            own_model: String::new(),
            own_vision: String::new(),
            server_url: String::new(),
            depth: Depth::Partial,
            photos: true,
            videos: true,
            documents: true,
            whole_pc: false,
            paused: false,
        }
    }
}

/// A download in progress. Mirrors `IndexDownload` in types.ts.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfo {
    pub done_bytes: u64,
    pub total_bytes: u64,
    /// Checking the files before they are used.
    pub verifying: bool,
}

/// Everything the UI shows about the index. Mirrors `IndexStatus` in types.ts.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub settings: IndexSettings,
    /// The chosen model can be used: downloaded and checked, or the person's own.
    pub ready: bool,
    pub download: Option<DownloadInfo>,
    /// The last problem, as an i18n key (`index.error.*`), maybe with `:detail`.
    pub error: Option<String>,
    /// Looking at files now.
    pub indexing: bool,
    /// Photos and videos in the index.
    pub indexed: u64,
    /// Still to look at in this round.
    pub pending: u64,
    /// The file being looked at.
    pub current: Option<String>,
    /// What the model, the engine and the index take on disk.
    pub disk_bytes: u64,
    /// What is still to download for the chosen model.
    pub download_bytes: u64,
}

/// What "Try" found out about a model. Mirrors `IndexCheck` in types.ts.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexCheck {
    pub text: bool,
    pub pictures: bool,
    /// Numbers per vector.
    pub dim: u32,
    /// The first problem, as an i18n key (`index.error.*`), maybe with `:detail`.
    pub error: Option<String>,
}

/// A model found on this PC (LM Studio, llama.cpp, Hugging Face). Mirrors `OwnModel` in types.ts.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OwnModel {
    pub model: String,
    /// The `mmproj` file beside it, when there is one: without it pictures cannot be read.
    pub vision: Option<String>,
    pub bytes: u64,
}

#[derive(Default)]
struct Inner {
    settings: Option<IndexSettings>,
    download: Option<DownloadInfo>,
    error: Option<String>,
    indexing: bool,
    pending: u64,
    current: Option<String>,
}

/// The index's state, managed by Tauri.
#[derive(Default)]
pub struct LocalIndex {
    inner: Mutex<Inner>,
    index: Mutex<Option<Index>>,
    /// One indexing round at a time.
    round: Mutex<()>,
    /// Bumped to stop the running round (new settings, pause, removal).
    generation: AtomicU64,
    cancel_download: AtomicBool,
    downloading: AtomicBool,
    last_emit: Mutex<Option<Instant>>,
}

// ── Where things are ─────────────────────────────────────────────────────────

fn home(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("LocalIndex"))
}

fn engine_dir(home: &Path) -> PathBuf {
    home.join("engine").join(ENGINE_VERSION)
}

/// The engine's server, once unpacked and found (its path is kept in `ok`).
fn engine_exe(home: &Path) -> Option<PathBuf> {
    let dir = engine_dir(home);
    let rel = fs::read_to_string(dir.join("ok")).ok()?;
    let exe = dir.join(rel.trim());
    exe.is_file().then_some(exe)
}

fn model_path(home: &Path, f: &Pinned) -> PathBuf {
    home.join("models").join(f.file)
}

/// A model file is ready once its checksum matched (the `.ok` beside it says which).
fn model_ok(home: &Path, f: &Pinned) -> bool {
    let path = model_path(home, f);
    path.metadata().is_ok_and(|m| m.len() == f.size)
        && fs::read_to_string(path.with_extension("ok")).is_ok_and(|s| s.trim() == f.sha256)
}

/// Bytes still to download for `quality` (the engine too, when it is missing).
fn missing_bytes(home: &Path, quality: Quality) -> u64 {
    let models: u64 = quality
        .files()
        .iter()
        .filter(|f| !model_ok(home, f))
        .map(|f| f.size)
        .sum();
    models
        + if engine_exe(home).is_none() {
            ENGINE.size
        } else {
            0
        }
}

/// The person's own server, when its address is one on this PC: pictures never leave it.
fn server_url(settings: &IndexSettings) -> Option<Url> {
    Url::parse(&settings.server_url).filter(|u| !u.https && u.is_loopback())
}

fn own_file(path: &str) -> Option<PathBuf> {
    let p = PathBuf::from(path.trim());
    let gguf = p
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
    (gguf && p.is_absolute() && p.is_file()).then_some(p)
}

/// Who turns pictures and words into vectors, once everything it needs is on this PC.
fn engine_for(home: &Path, settings: &IndexSettings) -> Option<engine::Engine> {
    match settings.source {
        Source::Download => {
            let [text, vision] = settings.quality.files();
            if !(model_ok(home, text) && model_ok(home, vision)) {
                return None;
            }
            Some(engine::Engine {
                backend: engine::Backend::Own(engine::Setup {
                    exe: engine_exe(home)?,
                    text: model_path(home, text),
                    vision: Some(model_path(home, vision)),
                    log: home.join("engine.log"),
                }),
                gemma: true,
            })
        }
        Source::Files => {
            let text = own_file(&settings.own_model)?;
            let vision = match settings.own_vision.trim() {
                "" => None,
                v => Some(own_file(v)?),
            };
            let gemma = text.file_name().is_some_and(|n| {
                n.to_string_lossy()
                    .to_lowercase()
                    .contains("embeddinggemma")
            });
            Some(engine::Engine {
                backend: engine::Backend::Own(engine::Setup {
                    exe: engine_exe(home)?,
                    text,
                    vision,
                    log: home.join("engine.log"),
                }),
                gemma,
            })
        }
        Source::Server => Some(engine::Engine {
            backend: engine::Backend::Server(server_url(settings)?),
            gemma: false,
        }),
    }
}

fn ready(home: &Path, settings: &IndexSettings) -> bool {
    engine_for(home, settings).is_some()
}

/// Which model made the vectors: an index made by another one starts over.
fn model_id(settings: &IndexSettings) -> String {
    match settings.source {
        Source::Download => settings.quality.id().to_string(),
        Source::Files => {
            let size = fs::metadata(settings.own_model.trim())
                .map(|m| m.len())
                .unwrap_or(0);
            format!("file:{}:{size}", settings.own_model.trim())
        }
        Source::Server => format!("server:{}", settings.server_url.trim()),
    }
}

/// Bytes still to download: the engine for the person's own files, nothing for their server.
fn missing_for(home: &Path, settings: &IndexSettings) -> u64 {
    match settings.source {
        Source::Download => missing_bytes(home, settings.quality),
        Source::Files if engine_exe(home).is_none() => ENGINE.size,
        Source::Files | Source::Server => 0,
    }
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    files::walk(dir, &|_| false, &mut |_, meta| total += meta.len());
    total
}

// ── State ────────────────────────────────────────────────────────────────────

impl LocalIndex {
    fn settings(&self, app: &AppHandle) -> IndexSettings {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.settings.is_none() {
            let saved = home(app)
                .ok()
                .and_then(|h| fs::read(h.join("settings.json")).ok())
                .and_then(|b| serde_json::from_slice(&b).ok());
            inner.settings = Some(saved.unwrap_or_default());
        }
        inner.settings.clone().unwrap_or_default()
    }

    fn save_settings(&self, app: &AppHandle, s: &IndexSettings) -> Result<(), String> {
        let h = home(app)?;
        fs::create_dir_all(&h).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
        fs::write(h.join("settings.json"), json).map_err(|e| e.to_string())?;
        self.inner.lock().map_err(|e| e.to_string())?.settings = Some(s.clone());
        Ok(())
    }

    fn status(&self, app: &AppHandle) -> IndexStatus {
        let settings = self.settings(app);
        let h = home(app).ok();
        // Loaded once and kept: searches need it too.
        let indexed = match (self.index.lock(), h.as_ref()) {
            (Ok(mut slot), Some(h)) => {
                let model = model_id(&settings);
                if slot.as_ref().is_none_or(|i| i.model != model) {
                    *slot = Some(Index::load_or_new(&h.join("index.bin"), &model));
                }
                slot.as_ref().map(|i| i.entries.len() as u64).unwrap_or(0)
            }
            _ => 0,
        };
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        IndexStatus {
            ready: h.as_ref().is_some_and(|h| ready(h, &settings)),
            disk_bytes: h.as_ref().map(|h| dir_size(h)).unwrap_or(0),
            download_bytes: h.as_ref().map(|h| missing_for(h, &settings)).unwrap_or(0),
            settings,
            download: inner.download.clone(),
            error: inner.error.clone(),
            indexing: inner.indexing,
            indexed,
            pending: inner.pending,
            current: inner.current.clone(),
        }
    }

    fn update(&self, f: impl FnOnce(&mut Inner)) {
        if let Ok(mut inner) = self.inner.lock() {
            f(&mut inner);
        }
    }
}

/// Tells the UI. `now` skips the pacing (a phase changed); otherwise at most a few times a second.
fn emit(app: &AppHandle, now: bool) {
    let state = app.state::<LocalIndex>();
    if !now {
        let mut last = state.last_emit.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|t| t.elapsed() < Duration::from_millis(400)) {
            return;
        }
        *last = Some(Instant::now());
    }
    let _ = app.emit(EVENT, state.status(app));
}

// ── Download ─────────────────────────────────────────────────────────────────

/// Downloads `f` into `downloads/`, carrying on a download stopped earlier, checks
/// it and returns where it is. Retries a few times when the connection drops.
fn fetch(
    app: &AppHandle,
    home: &Path,
    f: &Pinned,
    before: u64,
    total: u64,
) -> Result<PathBuf, String> {
    let state = app.state::<LocalIndex>();
    let dir = home.join("downloads");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let part = dir.join(format!("{}.part", f.file));
    let url = Url::parse(f.url).ok_or("index.error.download:url")?;
    let mut tries = 0;
    loop {
        let mut file = OpenOptions::new()
            .create(true)
            .append(false)
            .write(true)
            .truncate(false)
            .open(&part)
            .map_err(|e| e.to_string())?;
        let from = file.metadata().map(|m| m.len()).unwrap_or(0).min(f.size);
        let result = if from == f.size {
            Ok(())
        } else {
            http::download(&url, &mut file, from, &mut |done, _| {
                state.update(|i| {
                    i.download = Some(DownloadInfo {
                        done_bytes: before + done,
                        total_bytes: total,
                        verifying: false,
                    })
                });
                emit(app, false);
                !state.cancel_download.load(Ordering::SeqCst)
            })
        };
        drop(file);
        match result {
            Ok(()) => break,
            Err(_) if state.cancel_download.load(Ordering::SeqCst) => {
                return Err("cancelled".into())
            }
            Err(HttpError::Offline | HttpError::Timeout) if tries < 4 => {
                tries += 1;
                std::thread::sleep(Duration::from_secs(2 << tries));
            }
            Err(HttpError::Offline | HttpError::Timeout) => {
                return Err("index.error.offline".into())
            }
            Err(HttpError::Other(e)) => return Err(format!("index.error.download:{e}")),
        }
    }
    state.update(|i| {
        i.download = Some(DownloadInfo {
            done_bytes: before + f.size,
            total_bytes: total,
            verifying: true,
        })
    });
    emit(app, true);
    let digest = sha256::file_hex(&part, &mut |_| {}).map_err(|e| e.to_string())?;
    if digest != f.sha256 {
        let _ = fs::remove_file(&part);
        return Err(format!("index.error.badFile:{}", f.file));
    }
    Ok(part)
}

/// Unpacks llama.cpp with the `tar` that ships with Windows 10 and 11, and finds its server.
fn unpack_engine(home: &Path, zip: &Path) -> Result<(), String> {
    let dir = engine_dir(home);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    platform::unzip(zip, &dir)?;
    let mut exe = None;
    files::walk(&dir, &|_| false, &mut |p, _| {
        let is_server = p
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case(platform::SERVER));
        if is_server && exe.is_none() {
            exe = Some(p);
        }
    });
    let exe = exe.ok_or("index.error.download:no server in the archive")?;
    let rel = exe.strip_prefix(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("ok"), rel.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(zip);
    // Older engines are no longer needed.
    if let Ok(entries) = fs::read_dir(home.join("engine")) {
        for e in entries.flatten() {
            if e.file_name() != ENGINE_VERSION {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }
    Ok(())
}

fn download_all(app: &AppHandle, settings: &IndexSettings) -> Result<(), String> {
    let h = home(app)?;
    let models_dir = h.join("models");
    fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    let quality = settings.quality;
    let need_engine = engine_exe(&h).is_none() && settings.source != Source::Server;
    // The person's own model needs only the engine.
    let missing: Vec<&Pinned> = quality
        .files()
        .into_iter()
        .filter(|f| settings.source == Source::Download && !model_ok(&h, f))
        .collect();
    let total =
        missing.iter().map(|f| f.size).sum::<u64>() + if need_engine { ENGINE.size } else { 0 };
    let mut before = 0;
    if need_engine {
        let zip = fetch(app, &h, &ENGINE, before, total)?;
        unpack_engine(&h, &zip)?;
        before += ENGINE.size;
    }
    for f in missing {
        let part = fetch(app, &h, f, before, total)?;
        let dest = model_path(&h, f);
        let _ = fs::remove_file(&dest);
        fs::rename(&part, &dest).map_err(|e| e.to_string())?;
        fs::write(dest.with_extension("ok"), f.sha256).map_err(|e| e.to_string())?;
        before += f.size;
    }
    if settings.source != Source::Download {
        return Ok(());
    }
    // The files of another choice are no longer needed.
    let keep: Vec<&str> = quality.files().iter().map(|f| f.file).collect();
    for name in models::all_model_files() {
        if !keep.contains(&name) {
            let p = models_dir.join(name);
            let _ = fs::remove_file(&p);
            let _ = fs::remove_file(p.with_extension("ok"));
        }
    }
    Ok(())
}

// ── Indexing ─────────────────────────────────────────────────────────────────

/// Top-level Windows folders that hold no personal files.
const SYSTEM_DIRS: [&str; 14] = [
    "Windows",
    "Windows.old",
    "Program Files",
    "Program Files (x86)",
    "ProgramData",
    "System Volume Information",
    "Recovery",
    "PerfLogs",
    "MSOCache",
    "Config.Msi",
    "OneDriveTemp",
    "Intel",
    "AMD",
    "NVIDIA",
];

/// What the whole-PC walk leaves out: system folders at the top of a drive,
/// `$` folders (Recycle Bin, Windows setup), and what the user folders skip.
pub fn skip_dir(path: &Path) -> bool {
    if files::skip_personal_dir(path) {
        return true;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let at_top = path.parent().is_some_and(|p| p.parent().is_none());
    name.starts_with('$') || (at_top && SYSTEM_DIRS.iter().any(|d| d.eq_ignore_ascii_case(name)))
}

/// Where the index looks: the user's folders, or every fixed drive too.
fn roots(settings: &IndexSettings) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = files::profile_dir().into_iter().collect();
    if settings.whole_pc {
        let system = crate::disk::system_drive();
        dirs.push(PathBuf::from(format!(
            "{}\\",
            system.trim_end_matches(['\\', '/'])
        )));
        for d in crate::disk::other_drives() {
            dirs.push(PathBuf::from(format!("{}\\", d.mount_point)));
        }
    }
    files::dedup_dirs(&dirs)
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

struct Todo {
    path: PathBuf,
    kind: FileKind,
    size: u64,
    modified: u64,
}

/// The photos and videos to look at: what the settings ask for, new or changed
/// since the last round. Also drops from the index files that are gone.
fn plan(settings: &IndexSettings, index: &mut Index) -> Vec<Todo> {
    plan_in(&roots(settings), settings, index)
}

fn plan_in(roots: &[PathBuf], settings: &IndexSettings, index: &mut Index) -> Vec<Todo> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut todo = Vec::new();
    let mut walked = 0u64;
    for root in roots {
        files::walk(root, &skip_dir, &mut |path, meta| {
            walked += 1;
            if walked > MAX_WALK {
                return;
            }
            let kind = match kind_of(&path) {
                Some(FileKind::Image) if settings.photos && meta.len() >= MIN_PHOTO_BYTES => {
                    FileKind::Image
                }
                Some(FileKind::Video) if settings.videos => FileKind::Video,
                _ => return,
            };
            let modified = secs(meta.modified().unwrap_or(UNIX_EPOCH));
            let depth = if kind == FileKind::Video {
                settings.depth.code()
            } else {
                0
            };
            seen.insert(path.clone());
            let same = index.entries.get(&path).is_some_and(|e| {
                e.size == meta.len() && e.modified == modified && e.depth == depth
            });
            if !same {
                todo.push(Todo {
                    path,
                    kind,
                    size: meta.len(),
                    modified,
                });
            }
        });
    }
    if walked <= MAX_WALK {
        index.entries.retain(|p, _| seen.contains(p));
    }
    // The newest first: they are what people look for most.
    todo.sort_by_key(|t| std::cmp::Reverse(t.modified));
    todo
}

fn save_index(app: &AppHandle, h: &Path) {
    let state = app.state::<LocalIndex>();
    let index = state.index.lock();
    if let Ok(index) = index {
        if let Some(index) = index.as_ref() {
            let _ = index.save(&h.join("index.bin"));
        }
    };
}

/// One round: looks at every new or changed photo and video, then stops.
fn index_round(app: &AppHandle, generation: u64) -> Result<(), String> {
    let state = app.state::<LocalIndex>();
    let _round = state.round.lock().map_err(|e| e.to_string())?;
    let current = || state.generation.load(Ordering::SeqCst) == generation;
    let settings = state.settings(app);
    let h = home(app)?;
    if settings.paused || !current() {
        return Ok(());
    }
    let Some(engine) = engine_for(&h, &settings) else {
        return Ok(());
    };
    let model = model_id(&settings);
    let todo = {
        let mut slot = state.index.lock().map_err(|e| e.to_string())?;
        if slot.as_ref().is_none_or(|i| i.model != model) {
            *slot = Some(Index::load_or_new(&h.join("index.bin"), &model));
        }
        let index = slot.as_mut().ok_or("no index")?;
        plan(&settings, index)
    };
    state.update(|i| {
        i.indexing = true;
        i.pending = todo.len() as u64;
        i.error = None;
    });
    emit(app, true);
    let mut saved = Instant::now();
    let mut result = Ok(());
    for (n, item) in todo.iter().enumerate() {
        if !current() {
            break;
        }
        state.update(|i| {
            i.current = item
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            i.pending = (todo.len() - n) as u64;
        });
        emit(app, false);
        let pictures = frames::pictures(&item.path, item.kind, settings.depth);
        let mut each: Vec<Vec<i8>> = Vec::new();
        for png in &pictures {
            if !current() {
                break;
            }
            match engine::embed(&engine, &engine::Input::Picture(png)) {
                Ok(v) => each.extend(store::quantize(&v)),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        if result.is_err() {
            break;
        }
        if !current() {
            break;
        }
        if let Ok(mut slot) = state.index.lock() {
            if let Some(index) = slot.as_mut() {
                // The first vector sets the size; a model that changes it is not trusted.
                let dim = each.first().map_or(index.dim, Vec::len);
                if (index.dim != 0 && dim != index.dim) || each.iter().any(|v| v.len() != dim) {
                    result = Err("index.error.engine:size".into());
                    break;
                }
                index.dim = dim;
                let vectors = each.concat();
                index.entries.insert(
                    item.path.clone(),
                    store::Entry {
                        size: item.size,
                        modified: item.modified,
                        depth: if item.kind == FileKind::Video {
                            settings.depth.code()
                        } else {
                            0
                        },
                        vectors,
                    },
                );
            }
        }
        if saved.elapsed() > SAVE_EVERY {
            save_index(app, &h);
            saved = Instant::now();
        }
    }
    save_index(app, &h);
    state.update(|i| {
        i.indexing = false;
        i.pending = 0;
        i.current = None;
        if let Err(e) = &result {
            i.error = Some(e.clone());
        }
    });
    emit(app, true);
    result
}

/// Starts a new round in the background (the running one, if any, stops first).
fn start_round(app: &AppHandle) {
    let state = app.state::<LocalIndex>();
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = index_round(&app, generation);
    });
}

/// At start: a round for files added since the app last ran.
pub fn init(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(START_DELAY);
        start_round(&app);
    });
}

/// The app is closing: llama.cpp goes too.
pub fn on_exit() {
    engine::stop();
}

// ── Searching ────────────────────────────────────────────────────────────────

/// Files that match a topic, best first.
pub struct Found {
    /// Photos and videos with how close they are (from -1 to 1).
    pub media: Vec<(PathBuf, f32)>,
    /// Documents in Windows Search's order.
    pub documents: Vec<PathBuf>,
    /// Photos and videos not looked at yet: they may match too.
    pub not_indexed: u64,
}

/// Whether a topic search can run now (the model is downloaded).
pub fn can_search(app: &AppHandle) -> bool {
    let settings = app.state::<LocalIndex>().settings(app);
    home(app).is_ok_and(|h| ready(&h, &settings))
}

/// Looks for `topic` in the index (and Windows Search for documents). Blocking:
/// starting the model takes a few seconds the first time.
pub fn find(app: &AppHandle, topic: &str, media: bool, documents: bool) -> Result<Found, String> {
    let state = app.state::<LocalIndex>();
    let settings = state.settings(app);
    let h = home(app)?;
    let mut found = Found {
        media: Vec::new(),
        documents: Vec::new(),
        not_indexed: state.inner.lock().map(|i| i.pending).unwrap_or(0),
    };
    if media {
        let engine = engine_for(&h, &settings).ok_or("index.error.notReady")?;
        let query = engine::embed(&engine, &engine::Input::Query(topic))?;
        let mut slot = state.index.lock().map_err(|e| e.to_string())?;
        let model = model_id(&settings);
        if slot.as_ref().is_none_or(|i| i.model != model) {
            *slot = Some(Index::load_or_new(&h.join("index.bin"), &model));
        }
        if let Some(index) = slot.as_ref().filter(|i| i.dim == query.len()) {
            found.media = index
                .entries
                .iter()
                .filter_map(|(p, e)| Some((p.clone(), e.score(&query)?)))
                .collect();
        }
        found
            .media
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    }
    if documents && settings.documents {
        found.documents = docs::search(topic, &roots(&settings));
    }
    Ok(found)
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn index_status(app: AppHandle) -> IndexStatus {
    app.state::<LocalIndex>().status(&app)
}

/// Saves new choices. A new model needs a download first; the rest starts a new round.
#[tauri::command]
pub fn index_set_settings(app: AppHandle, settings: IndexSettings) -> Result<IndexStatus, String> {
    let state = app.state::<LocalIndex>();
    let before = state.settings(&app);
    state.save_settings(&app, &settings)?;
    let changed_model = model_id(&before) != model_id(&settings);
    let changed_files = changed_model
        || before.depth != settings.depth
        || before.photos != settings.photos
        || before.videos != settings.videos
        || before.whole_pc != settings.whole_pc;
    if settings.paused {
        state.generation.fetch_add(1, Ordering::SeqCst);
    } else if changed_files || before.paused {
        start_round(&app);
    }
    if changed_model {
        if let Ok(mut slot) = state.index.lock() {
            *slot = None;
        }
    }
    let status = state.status(&app);
    let _ = app.emit(EVENT, status.clone());
    Ok(status)
}

/// Downloads the engine and the chosen model in the background, then starts indexing.
#[tauri::command]
pub fn index_install(app: AppHandle) -> Result<IndexStatus, String> {
    let state = app.state::<LocalIndex>();
    if state.downloading.swap(true, Ordering::SeqCst) {
        return Ok(state.status(&app));
    }
    state.cancel_download.store(false, Ordering::SeqCst);
    let settings = state.settings(&app);
    state.update(|i| {
        i.error = None;
        i.download = Some(DownloadInfo {
            done_bytes: 0,
            total_bytes: 0,
            verifying: false,
        });
    });
    let worker = app.clone();
    std::thread::spawn(move || {
        let result = download_all(&worker, &settings);
        let state = worker.state::<LocalIndex>();
        state.update(|i| {
            i.download = None;
            match &result {
                Err(e) if e != "cancelled" => i.error = Some(e.clone()),
                _ => {}
            }
        });
        state.downloading.store(false, Ordering::SeqCst);
        emit(&worker, true);
        if result.is_ok() {
            start_round(&worker);
        }
    });
    let status = state.status(&app);
    let _ = app.emit(EVENT, status.clone());
    Ok(status)
}

/// Tries a model before it is used: a few words, then a small picture. Starting
/// a model takes a while the first time.
#[tauri::command]
pub async fn index_check(app: AppHandle, settings: IndexSettings) -> Result<IndexCheck, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let h = home(&app)?;
        let mut check = IndexCheck {
            text: false,
            pictures: false,
            dim: 0,
            error: None,
        };
        let engine = match engine_for(&h, &settings) {
            Some(e) => e,
            None if settings.source == Source::Files && engine_exe(&h).is_none() => {
                check.error = Some("index.error.needEngine".into());
                return Ok(check);
            }
            None => {
                check.error = Some("index.error.notFound".into());
                return Ok(check);
            }
        };
        let words = match engine::embed(&engine, &engine::Input::Query("holidays at the sea")) {
            Ok(v) => v,
            Err(e) => {
                check.error = Some(e);
                return Ok(check);
            }
        };
        check.text = true;
        check.dim = words.len() as u32;
        if let engine::Backend::Own(engine::Setup { vision: None, .. }) = engine.backend {
            check.error = Some("index.error.noVision".into());
            return Ok(check);
        }
        match engine::embed(&engine, &engine::Input::Picture(&test_picture())) {
            Ok(v) if v.len() == words.len() => check.pictures = true,
            Ok(_) => check.error = Some("index.error.engine:size".into()),
            Err(e) => check.error = Some(e),
        }
        Ok(check)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A small picture for the check: a blue sky over a yellow beach.
fn test_picture() -> Vec<u8> {
    let side = 64u32;
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for _ in 0..side {
            let px = if y < side * 2 / 3 {
                [90, 160, 230, 255]
            } else {
                [235, 210, 140, 255]
            };
            rgba.extend_from_slice(&px);
        }
    }
    crate::apps::icon::encode_png(side, side, &rgba)
}

/// Where other apps keep the models they downloaded.
fn model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in ["LLAMA_CACHE", "LMSTUDIO_MODELS"] {
        if let Some(d) = std::env::var_os(var) {
            dirs.push(PathBuf::from(d));
        }
    }
    if let Some(d) = std::env::var_os("HF_HOME") {
        dirs.push(PathBuf::from(d).join("hub"));
    }
    if let Some(d) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(d).join("llama.cpp"));
    }
    if let Some(p) = files::profile_dir() {
        for rel in [
            ".lmstudio/models",
            ".cache/lm-studio/models",
            ".cache/llama.cpp",
            ".cache/huggingface/hub",
        ] {
            dirs.push(p.join(rel));
        }
    }
    dirs.retain(|d| d.is_dir());
    files::dedup_dirs(&dirs)
}

/// Embedding models (`.gguf` with "embed" in the name) in `dirs`, each with the
/// `mmproj` file beside it, if any.
fn models_in(dirs: &[PathBuf]) -> Vec<OwnModel> {
    let mut found: Vec<(PathBuf, u64)> = Vec::new();
    for dir in dirs {
        let mut walked = 0u32;
        files::walk(dir, &|_| false, &mut |path, meta| {
            walked += 1;
            let gguf = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
            if walked < 200_000 && gguf {
                found.push((path, meta.len()));
            }
        });
    }
    let name = |p: &Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    };
    let mut out: Vec<OwnModel> = found
        .iter()
        .filter(|(p, _)| {
            let n = name(p);
            n.contains("embed") && !n.starts_with("mmproj")
        })
        .map(|(p, bytes)| OwnModel {
            model: p.to_string_lossy().into_owned(),
            vision: found
                .iter()
                .filter(|(v, _)| v.parent() == p.parent() && name(v).starts_with("mmproj"))
                .map(|(v, _)| v.to_string_lossy().into_owned())
                .min(),
            bytes: *bytes,
        })
        .collect();
    out.sort_by(|a, b| a.model.cmp(&b.model));
    out.dedup_by(|a, b| a.model == b.model);
    out
}

/// Embedding models other apps already downloaded (LM Studio, llama.cpp, Hugging Face).
#[tauri::command]
pub async fn index_find_models() -> Vec<OwnModel> {
    tauri::async_runtime::spawn_blocking(|| models_in(&model_dirs()))
        .await
        .unwrap_or_default()
}

/// Stops the download; what came already is kept for the next try.
#[tauri::command]
pub fn index_cancel_download(app: AppHandle) {
    app.state::<LocalIndex>()
        .cancel_download
        .store(true, Ordering::SeqCst);
}

/// Deletes the model, the engine and the index. Settings stay.
#[tauri::command]
pub async fn index_remove(app: AppHandle) -> Result<IndexStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LocalIndex>();
        state.cancel_download.store(true, Ordering::SeqCst);
        state.generation.fetch_add(1, Ordering::SeqCst);
        engine::stop();
        // Waits for the round to stop before the files go.
        let _round = state.round.lock().map_err(|e| e.to_string())?;
        if let Ok(mut slot) = state.index.lock() {
            *slot = None;
        }
        let h = home(&app)?;
        let settings = state.settings(&app);
        for name in [
            "engine",
            "models",
            "downloads",
            "index.bin",
            "index.tmp",
            "engine.log",
        ] {
            let p = h.join(name);
            let _ = if p.is_dir() {
                fs::remove_dir_all(&p)
            } else {
                fs::remove_file(&p)
            };
        }
        state.update(|i| {
            i.error = None;
            i.pending = 0;
            i.current = None;
        });
        let _ = state.save_settings(&app, &settings);
        let status = state.status(&app);
        let _ = app.emit(EVENT, status.clone());
        Ok(status)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    pub const SERVER: &str = "llama-server.exe";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn unzip(zip: &Path, dir: &Path) -> Result<(), String> {
        let system = std::env::var_os("SystemRoot").ok_or("no SystemRoot")?;
        let tar = Path::new(&system).join(r"System32\tar.exe");
        let status = Command::new(tar)
            .arg("-xf")
            .arg(zip)
            .arg("-C")
            .arg(dir)
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("index.error.download:{e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err("index.error.download:unzip".into())
        }
    }
}

/// Easy Uninstaller targets Windows; elsewhere the engine cannot be unpacked.
#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    pub const SERVER: &str = "llama-server";

    pub fn unzip(_zip: &Path, _dir: &Path) -> Result<(), String> {
        Err("index.error.download:unsupported".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_models_are_found_with_their_picture_part() {
        let dir = std::env::temp_dir().join(format!("eu-models-{}", std::process::id()));
        let lm = dir.join("lmstudio-community").join("embeddinggemma-2-GGUF");
        let chat = dir.join("other").join("chat-GGUF");
        fs::create_dir_all(&lm).unwrap();
        fs::create_dir_all(&chat).unwrap();
        fs::write(lm.join("embeddinggemma-2-Q4_K_M.gguf"), b"m").unwrap();
        fs::write(lm.join("mmproj-embeddinggemma-2-F16.gguf"), b"v").unwrap();
        fs::write(chat.join("chat-Q4.gguf"), b"c").unwrap();
        fs::write(chat.join("nomic-embed-text.gguf"), b"e").unwrap();
        let found = models_in(std::slice::from_ref(&dir));
        assert_eq!(found.len(), 2, "chat models are left out: {found:?}");
        let gemma = found
            .iter()
            .find(|m| m.model.contains("embeddinggemma"))
            .unwrap();
        assert!(gemma.vision.as_deref().unwrap().contains("mmproj"));
        let nomic = found.iter().find(|m| m.model.contains("nomic")).unwrap();
        assert_eq!(nomic.vision, None, "no picture part beside it");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_server_must_be_on_this_pc() {
        let with = |url: &str| IndexSettings {
            source: Source::Server,
            server_url: url.into(),
            ..IndexSettings::default()
        };
        assert!(server_url(&with("http://127.0.0.1:8080")).is_some());
        assert!(server_url(&with("http://localhost:8080/v1")).is_some());
        assert!(server_url(&with("http://192.168.1.5:8080")).is_none());
        assert!(server_url(&with("https://example.com")).is_none());
        assert_ne!(
            model_id(&with("http://127.0.0.1:8080")),
            Quality::Light.id()
        );
    }

    #[test]
    fn system_folders_are_skipped_only_at_the_top() {
        // A drive's top folder, written the way this platform does.
        let top = |rest: &str| -> PathBuf {
            let root = if cfg!(windows) { "C:\\" } else { "/" };
            rest.split('/')
                .fold(PathBuf::from(root), |p, part| p.join(part))
        };
        assert!(skip_dir(&top("Windows")));
        assert!(skip_dir(&top("$RECYCLE.BIN")));
        assert!(skip_dir(&top("Program Files (x86)")));
        assert!(skip_dir(&top("Users/a/AppData")));
        assert!(skip_dir(&top("Foto/$tmp")));
        assert!(!skip_dir(&top("Foto/Windows")));
        assert!(!skip_dir(&top("Foto")));
    }

    #[test]
    fn settings_have_safe_defaults_and_read_old_files() {
        let s: IndexSettings = serde_json::from_str("{\"quality\": \"best\"}").unwrap();
        assert_eq!(s.quality, Quality::Best);
        assert_eq!(s.depth, Depth::Partial);
        assert!(s.photos && s.videos && s.documents);
        assert!(!s.whole_pc && !s.paused);
    }

    #[test]
    fn the_plan_takes_new_and_changed_files_and_forgets_gone_ones() {
        let dir = std::env::temp_dir().join(format!("eu-plan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Pictures")).unwrap();
        fs::write(dir.join("Pictures/beach.jpg"), vec![1u8; 40_000]).unwrap();
        fs::write(dir.join("Pictures/icon.png"), vec![1u8; 100]).unwrap();
        fs::write(dir.join("W20251201RM.mp4"), vec![1u8; 10]).unwrap();
        fs::write(dir.join("notes.txt"), b"x").unwrap();
        let settings = IndexSettings::default();
        let mut index = Index::new("m");
        index.entries.insert(
            dir.join("gone.jpg"),
            store::Entry {
                size: 1,
                modified: 1,
                depth: 0,
                vectors: Vec::new(),
            },
        );
        let todo = plan_in(std::slice::from_ref(&dir), &settings, &mut index);
        let mut names: Vec<&str> = todo
            .iter()
            .map(|t| t.path.file_name().unwrap().to_str().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["W20251201RM.mp4", "beach.jpg"],
            "no icons, no text files"
        );
        assert!(
            index.entries.is_empty(),
            "a file that is gone leaves the index"
        );

        // Once looked at, a file is not looked at again until it changes.
        for t in &todo {
            index.entries.insert(
                t.path.clone(),
                store::Entry {
                    size: t.size,
                    modified: t.modified,
                    depth: if t.kind == FileKind::Video {
                        settings.depth.code()
                    } else {
                        0
                    },
                    vectors: Vec::new(),
                },
            );
        }
        assert!(plan_in(std::slice::from_ref(&dir), &settings, &mut index).is_empty());
        let deeper = IndexSettings {
            depth: Depth::Full,
            ..settings.clone()
        };
        let again = plan_in(std::slice::from_ref(&dir), &deeper, &mut index);
        assert_eq!(again.len(), 1, "a deeper look takes the videos again");
        let no_videos = IndexSettings {
            videos: false,
            ..settings
        };
        assert!(plan_in(std::slice::from_ref(&dir), &no_videos, &mut index).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
