//! AI help for the folders the app does not know (the "unknown folders" group).
//!
//! Off until the user turns it on. The user picks where the model runs and who
//! provides it: a subscription through its own program (`cli.rs`), an online
//! service with an API key, or a model server on this PC (`services.rs`).
//!
//! The rules that keep it safe:
//! - only folders of the unknown-folders group are asked about, checked again
//!   here like a move (`app_folders::check`); built-in explanations always win;
//! - only names and numbers are sent (`prompt.rs`), never file contents;
//! - the answer is a fixed JSON object, checked field by field; it is advice,
//!   shown apart from what the app checked on this PC, and it never moves or
//!   ticks anything by itself;
//! - when an installed app has the folder's name (or the one the AI names),
//!   "probably removable" becomes "better keep it";
//! - answers are kept on this PC so the same folder is not asked twice.

mod cli;
pub(crate) mod http;
mod keys;
pub(crate) mod prompt;
mod services;
mod unsloth;

use crate::app_folders;
use crate::explain::KnownFolders;
use crate::leftovers::{clean_names, norm};
use prompt::{AiVerdict, Confidence, FolderSummary};
use serde::{Deserialize, Serialize};
use services::Kind;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Manager, State};

/// The roots unknown folders live in, by the variable that names them.
const ROOTS: [&str; 3] = ["LOCALAPPDATA", "APPDATA", "ProgramData"];

/// Which service and model the user picked (kept by the UI in its settings).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    pub service: String,
    #[serde(default)]
    pub model: String,
    /// For "other service" and servers on this PC: their address.
    #[serde(default)]
    pub url: String,
    /// Start Unsloth Studio when it is not running, and stop it after a while.
    #[serde(default = "yes")]
    pub auto_start: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiModel {
    pub id: String,
    pub label: String,
}

/// Whether a service can be used right now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiServiceStatus {
    pub id: String,
    /// The program is installed, or the server on this PC answers.
    pub found: bool,
    /// How many models the server on this PC has, when it says.
    pub models: Option<usize>,
    /// A key is saved for it.
    pub has_key: bool,
    /// The app can start it when it is not running (Unsloth Studio, installed).
    pub can_start: bool,
}

/// What the app checked on this PC, shown apart from the AI's answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AiFacts {
    /// An installed app whose name matches the folder (or the AI's guess).
    pub installed_app: Option<String>,
    /// The newest change inside, in seconds since 1970.
    pub modified: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiAnswer {
    pub path: String,
    pub what: String,
    pub made_by: String,
    pub verdict: AiVerdict,
    pub confidence: Confidence,
    pub reason: String,
    pub label: String,
    /// The AI said "removable" but an installed app matches: shown as "keep".
    pub overridden: bool,
    pub facts: AiFacts,
    pub service: String,
    pub model: String,
    pub asked_at: u64,
}

/// What would be sent for a folder, shown before sending.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPreview {
    pub summary: FolderSummary,
    /// The exact text sent.
    pub question: String,
}

/// Answers kept on this PC, and how many questions were sent this month.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Store {
    answers: HashMap<String, AiAnswer>,
    month: String,
    asked: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStats {
    pub answers: usize,
    pub asked_this_month: u32,
}

/// The answers file, loaded on first use.
#[derive(Default)]
pub struct Answers(Mutex<Option<Store>>);

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("ai-answers.json"))
}

impl Answers {
    fn with<T>(&self, app: &AppHandle, f: impl FnOnce(&mut Store) -> T) -> Result<T, String> {
        let mut guard = self.0.lock().map_err(|e| e.to_string())?;
        let store = guard.get_or_insert_with(|| {
            store_path(app)
                .ok()
                .and_then(|p| std::fs::read(p).ok())
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default()
        });
        Ok(f(store))
    }

    /// Written while the store is locked, so questions answered at once do not
    /// write the file over each other.
    fn save(&self, app: &AppHandle) -> Result<(), String> {
        let path = store_path(app)?;
        self.with(app, |s| {
            let text = serde_json::to_vec(s).map_err(|e| e.to_string())?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
        })?
    }
}

fn env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// Checks `path` like a move would, and that the app has no explanation of its
/// own for it. Returns how it is shown (`%LOCALAPPDATA%\Name`).
fn checked(app: &AppHandle, path: &Path) -> Result<String, String> {
    let roots: Vec<(&str, PathBuf)> = ROOTS.iter().filter_map(|v| Some((*v, env(v)?))).collect();
    let root_dirs: Vec<PathBuf> = roots.iter().map(|(_, p)| p.clone()).collect();
    let own = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    let known = KnownFolders::from_env();
    app_folders::check(path, &root_dirs, &known, &own)
        .map_err(|_| "ai.error.refused".to_string())?;
    if known.lookup(path).is_some() {
        return Err("ai.error.refused".into());
    }
    let refs: Vec<(&str, &Path)> = roots.iter().map(|(v, p)| (*v, p.as_path())).collect();
    prompt::shown_path(path, &refs).ok_or_else(|| "ai.error.refused".into())
}

fn user_name() -> String {
    std::env::var("USERNAME").unwrap_or_default()
}

/// An installed app whose name matches one of `names` (the folder's, the AI's guess).
pub fn installed_match(names: &[&str], installed: &[String]) -> Option<String> {
    let wanted: Vec<String> = names
        .iter()
        .map(|n| norm(n))
        .filter(|n| n.len() >= 4)
        .collect();
    installed
        .iter()
        .find(|app| {
            clean_names(app, "").iter().any(|c| {
                wanted.iter().any(|w| {
                    w == c || (c.len() >= 4 && w.contains(c.as_str())) || c.contains(w.as_str())
                })
            })
        })
        .cloned()
}

/// Starts Unsloth Studio first when it is the picked service and is not running.
fn ensure_running(app: &AppHandle, config: &AiConfig) -> Result<(), String> {
    if config.service == unsloth::ID {
        unsloth::ensure(app, &config.url, config.auto_start)?;
    }
    Ok(())
}

/// One question to the picked service, blocking. Returns the raw answer text.
fn send(app: &AppHandle, config: &AiConfig, question: &str) -> Result<String, String> {
    let service = services::find(&config.service).ok_or("ai.error.noService")?;
    ensure_running(app, config)?;
    let _busy = unsloth::busy();
    match service.kind {
        Kind::Cli => cli::ask(service.id, &config.model, question),
        _ => services::ask(service, &config.url, &config.model, question),
    }
}

/// One question for another AI feature (finding files by a sentence, tidying
/// Downloads), counted with this month's questions. Returns the raw answer text.
pub(crate) fn ask_text(
    app: &AppHandle,
    config: &AiConfig,
    question: &str,
) -> Result<String, String> {
    let text = send(app, config, question)?;
    let answers = app.state::<Answers>();
    let month = prompt::year_month(prompt::now_secs());
    answers.with(app, |s| {
        if s.month != month {
            s.month = month;
            s.asked = 0;
        }
        s.asked += 1;
    })?;
    answers.save(app)?;
    Ok(text)
}

/// The app is closing: the Unsloth Studio it started goes too.
pub fn on_exit() {
    unsloth::stop_owned();
}

/// The services and whether each can be used: programs installed, servers on
/// this PC answering, keys saved. `urls` are the user's addresses, by service.
#[tauri::command]
pub async fn ai_services(urls: HashMap<String, String>) -> Result<Vec<AiServiceStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let probes: Vec<_> = services::SERVICES
            .iter()
            .map(|s| {
                let url = urls.get(s.id).cloned().unwrap_or_default();
                std::thread::spawn(move || {
                    let (found, models) = match s.kind {
                        Kind::Cli => (!cli::candidates(s.id).is_empty(), None),
                        Kind::Local => match services::probe(s, &url) {
                            Some(n) => (true, n),
                            None => (false, None),
                        },
                        Kind::Api => (true, None),
                    };
                    AiServiceStatus {
                        id: s.id.to_string(),
                        found,
                        models,
                        has_key: keys::get(s.id).is_some(),
                        can_start: s.id == unsloth::ID && unsloth::can_start(),
                    }
                })
            })
            .collect();
        probes
            .into_iter()
            .map(|p| p.join().map_err(|_| "probe failed".to_string()))
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ai_models(app: AppHandle, config: AiConfig) -> Result<Vec<AiModel>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let service = services::find(&config.service).ok_or("ai.error.noService")?;
        ensure_running(&app, &config)?;
        match service.kind {
            Kind::Cli => cli::models(service.id),
            _ => services::models(service, &config.url),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn ai_set_key(service: String, key: String) -> Result<(), String> {
    services::find(&service).ok_or("ai.error.noService")?;
    keys::set(&service, &key)
}

#[tauri::command]
pub fn ai_remove_key(service: String) -> Result<(), String> {
    services::find(&service).ok_or("ai.error.noService")?;
    keys::remove(&service);
    Ok(())
}

/// Sends a tiny question; returns how long the answer took, in milliseconds.
#[tauri::command]
pub async fn ai_test(app: AppHandle, config: AiConfig) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_running(&app, &config)?;
        let started = Instant::now();
        let text = send(&app, &config, prompt::TEST_QUESTION)?;
        prompt::answer_object(&text).ok_or("ai.error.badAnswer")?;
        Ok(started.elapsed().as_millis() as u64)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What would be sent for `path`.
#[tauri::command]
pub async fn ai_preview(app: AppHandle, path: String, lang: String) -> Result<AiPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(path);
        let shown = checked(&app, &path)?;
        let summary = prompt::summarize(&path, shown, &user_name());
        let question = prompt::question(&summary, &lang);
        Ok(AiPreview { summary, question })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Asks about `path` (the folder is looked at again here, nothing the UI sends
/// is passed on), keeps the answer and returns it.
#[tauri::command]
pub async fn ai_ask(
    app: AppHandle,
    config: AiConfig,
    path: String,
    lang: String,
) -> Result<AiAnswer, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(&path);
        let shown = checked(&app, &path)?;
        let summary = prompt::summarize(&path, shown, &user_name());
        let question = prompt::question(&summary, &lang);
        let text = send(&app, &config, &question)?;
        let reply = prompt::parse_reply(&text).ok_or("ai.error.badAnswer")?;

        let folder = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let installed = crate::files::installed_app_names();
        let installed_app = installed_match(&[&folder, &reply.made_by], &installed);
        let overridden = reply.verdict == AiVerdict::Removable && installed_app.is_some();
        let answer = AiAnswer {
            path: path.to_string_lossy().into_owned(),
            what: reply.what,
            made_by: reply.made_by,
            verdict: if overridden {
                AiVerdict::Keep
            } else {
                reply.verdict
            },
            confidence: reply.confidence,
            reason: reply.reason,
            label: reply.label,
            overridden,
            facts: AiFacts {
                installed_app,
                modified: summary.modified,
            },
            service: config.service.clone(),
            model: config.model.clone(),
            asked_at: prompt::now_secs(),
        };
        let answers = app.state::<Answers>();
        let month = prompt::year_month(answer.asked_at);
        answers.with(&app, |s| {
            if s.month != month {
                s.month = month;
                s.asked = 0;
            }
            s.asked += 1;
            s.answers.insert(key(&path), answer.clone());
        })?;
        answers.save(&app)?;
        Ok(answer)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The answers kept for `paths` (the folders on screen).
#[tauri::command]
pub fn ai_saved_answers(
    app: AppHandle,
    answers: State<'_, Answers>,
    paths: Vec<String>,
) -> Result<Vec<AiAnswer>, String> {
    answers.with(&app, |s| {
        paths
            .iter()
            .filter_map(|p| s.answers.get(&key(Path::new(p))).cloned())
            .collect()
    })
}

#[tauri::command]
pub fn ai_stats(app: AppHandle, answers: State<'_, Answers>) -> Result<AiStats, String> {
    let month = prompt::year_month(prompt::now_secs());
    answers.with(&app, |s| AiStats {
        answers: s.answers.len(),
        asked_this_month: if s.month == month { s.asked } else { 0 },
    })
}

/// Forgets kept answers: those of `paths`, or all of them.
#[tauri::command]
pub fn ai_forget_answers(
    app: AppHandle,
    answers: State<'_, Answers>,
    paths: Option<Vec<String>>,
) -> Result<(), String> {
    answers.with(&app, |s| match paths {
        Some(paths) => {
            for p in paths {
                s.answers.remove(&key(Path::new(&p)));
            }
        }
        None => s.answers.clear(),
    })?;
    answers.save(&app)
}

/// Help pages the UI may open: where to get a key, how to install a program.
const PAGES: &[(&str, &str)] = &[
    (
        "key:anthropic",
        "https://console.anthropic.com/settings/keys",
    ),
    ("key:openai", "https://platform.openai.com/api-keys"),
    ("key:google", "https://aistudio.google.com/apikey"),
    ("key:openrouter", "https://openrouter.ai/keys"),
    (
        "install:claude-code",
        "https://code.claude.com/docs/en/setup",
    ),
    ("install:codex", "https://developers.openai.com/codex/cli"),
    ("install:antigravity-cli", "https://antigravity.google/"),
    ("install:ollama", "https://ollama.com/download"),
    ("install:lmstudio", "https://lmstudio.ai/"),
    ("install:unsloth", "https://unsloth.ai/"),
];

/// Opens one of the help pages in the browser. Only these fixed addresses.
#[tauri::command]
pub fn ai_open_page(page: String) -> Result<(), String> {
    let url = PAGES
        .iter()
        .find(|(id, _)| *id == page)
        .map(|(_, url)| *url)
        .ok_or("unknown page")?;
    open_url(url);
    Ok(())
}

/// Opens a link with Windows (the default browser, or the app that handles it).
#[cfg(windows)]
pub(crate) fn open_url(url: &str) {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    // SAFETY: plain strings that outlive the call.
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(url),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(not(windows))]
pub(crate) fn open_url(_url: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fixed_https_pages_open() {
        assert!(PAGES.iter().all(|(_, u)| u.starts_with("https://")));
        assert!(ai_open_page("key:anthropic".into()).is_ok());
        assert!(ai_open_page("https://evil.example".into()).is_err());
    }

    #[test]
    fn installed_apps_are_matched_by_name() {
        let installed = vec![
            "Genshin Impact".to_string(),
            "qBittorrent 5.1.2 (x64)".to_string(),
            "Zoom Workplace".to_string(),
        ];
        assert_eq!(
            installed_match(&["qbt-data", "qBittorrent"], &installed).as_deref(),
            Some("qBittorrent 5.1.2 (x64)")
        );
        assert_eq!(
            installed_match(&["ABX_7731", "HoYoverse"], &installed),
            None
        );
        assert_eq!(
            installed_match(&["GenshinImpact", ""], &installed).as_deref(),
            Some("Genshin Impact")
        );
        // Short or empty names never match everything.
        assert_eq!(installed_match(&["", "x"], &installed), None);
    }

    #[test]
    fn every_service_has_a_known_kind() {
        assert_eq!(
            services::SERVICES
                .iter()
                .filter(|s| s.kind == Kind::Cli)
                .count(),
            3
        );
        assert_eq!(
            services::SERVICES
                .iter()
                .filter(|s| s.kind == Kind::Api)
                .count(),
            5
        );
        assert_eq!(
            services::SERVICES
                .iter()
                .filter(|s| s.kind == Kind::Local)
                .count(),
            4
        );
    }
}
