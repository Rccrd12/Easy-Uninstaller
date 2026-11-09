//! Unsloth Studio started by the app when a question needs it, and stopped again
//! once it has been idle for a while (the service's "Start and stop it by itself"
//! switch, on by default).
//!
//! Unsloth Studio's installer puts a Python of its own in
//! `%USERPROFILE%\.unsloth\studio\unsloth_studio`, and its desktop app starts the
//! server through that interpreter rather than the unsigned `unsloth.exe`, which
//! Windows Application Control may block. The app does the same:
//! `python.exe -X utf8 -c <its CLI's entry> studio -H 127.0.0.1 -p <port> --silent`,
//! with no window, on this PC only.
//!
//! The app only ever stops a server it started itself: one the user opened stays
//! as it is. Nothing runs while Unsloth Studio is not needed: the idle watcher
//! starts with the server and ends with it.

use super::cli::platform;
use super::http::{self, Url};
use super::{keys, services};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// The UI follows a start through this event.
pub const EVENT: &str = "ai-unsloth";

/// The service id in `services::SERVICES`.
pub const ID: &str = "unsloth";

/// The entry of Unsloth's CLI through its own interpreter, as its installer's
/// `unsloth.cmd` and its desktop app run it.
const ENTRY: &str = "import sys, os; sys.path[:1] = [x for x in sys.path[:1] if getattr(sys.flags, 'safe_path', False) or x not in ('', os.getcwd())]; sys.argv[0] = 'unsloth'; from unsloth_cli import app; sys.exit(app())";

/// The longest a start may take: the first one after an update installs things.
const START_LIMIT: Duration = Duration::from_secs(180);
/// How often a start looks whether the server answers yet.
const START_POLL: Duration = Duration::from_millis(500);
/// Stopped after this long with no question.
const IDLE_LIMIT: Duration = Duration::from_secs(5 * 60);
/// How often the idle watcher looks.
const IDLE_POLL: Duration = Duration::from_secs(15);
/// How long "does it answer?" waits.
const ANSWER_TIMEOUT: Duration = Duration::from_millis(1500);

/// The server the app started.
struct Owned {
    child: Child,
    /// It answered: the start is over.
    ready: bool,
    /// The last time a question needed it.
    last_used: Instant,
    /// Questions being answered with it now: never stopped under one.
    busy: usize,
}

static OWNED: Mutex<Option<Owned>> = Mutex::new(None);
/// One start at a time: a second caller waits for the first one's server.
static STARTING: Mutex<()> = Mutex::new(());

/// What the UI hears during a start.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartStatus {
    /// "starting", "on" or "off".
    pub state: &'static str,
    /// How long the start has taken so far.
    pub seconds: u64,
}

// ── Where it is ──────────────────────────────────────────────────────────────

/// Unsloth Studio's folder, as its CLI finds it: UNSLOTH_STUDIO_HOME (or
/// STUDIO_HOME), else UNSLOTH_HOME\studio once an install is there, else
/// %USERPROFILE%\.unsloth\studio.
fn studio_home() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    for name in ["UNSLOTH_STUDIO_HOME", "STUDIO_HOME"] {
        if let Some(dir) = var(name) {
            return Some(dir);
        }
    }
    let legacy = var("USERPROFILE").map(|h| h.join(".unsloth").join("studio"));
    if let Some(master) = var("UNSLOTH_HOME") {
        let candidate = master.join("studio");
        if python_in(&candidate).is_some() || legacy.as_deref().and_then(python_in).is_none() {
            return Some(candidate);
        }
    }
    legacy
}

/// The interpreter of the install at `home`: its venv, or the one older installs had.
fn python_in(home: &Path) -> Option<PathBuf> {
    ["unsloth_studio", ".venv"]
        .into_iter()
        .map(|venv| home.join(venv).join("Scripts").join("python.exe"))
        .find(|p| p.is_file())
}

/// Unsloth Studio's Python, if it is installed on this PC.
fn python() -> Option<PathBuf> {
    python_in(&studio_home()?)
}

/// Whether the app could start Unsloth Studio (it is installed).
pub fn can_start() -> bool {
    python().is_some()
}

/// The address the app would start it at: the user's, or the usual one, when
/// it is this PC over http.
fn start_address(typed: &str) -> Option<Url> {
    let service = services::find(ID)?;
    let url = services::address(service, typed).ok()?.into_iter().next()?;
    (!url.https && url.is_loopback()).then_some(url)
}

/// Whether a server answers at `base`, whatever it says (no key needed to know).
fn answers(base: &Url) -> bool {
    let headers = [("Accept", "application/json".to_string())];
    http::request("GET", &base.join("models"), &headers, None, ANSWER_TIMEOUT).is_ok()
}

/// Whether any of the addresses the app tries for Unsloth Studio answers.
fn any_answers(typed: &str) -> bool {
    services::find(ID)
        .and_then(|s| services::address(s, typed).ok())
        .is_some_and(|urls| urls.iter().any(answers))
}

// ── Its state ────────────────────────────────────────────────────────────────

fn owned<R>(f: impl FnOnce(&mut Option<Owned>) -> R) -> R {
    f(&mut OWNED.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Whether the server the app started has ended by itself (closed, crashed);
/// it is forgotten then.
fn owned_ended() -> bool {
    owned(|o| {
        let ended = o
            .as_mut()
            .is_some_and(|own| !matches!(own.child.try_wait(), Ok(None)));
        if ended {
            *o = None;
        }
        ended
    })
}

fn emit(app: &AppHandle, state: &'static str, seconds: u64) {
    let _ = app.emit(EVENT, StartStatus { state, seconds });
}

/// Held while a question is answered with Unsloth Studio: it is not stopped under one.
pub struct Busy(bool);

pub fn busy() -> Busy {
    Busy(owned(|o| match o {
        Some(own) => {
            own.busy += 1;
            own.last_used = Instant::now();
            true
        }
        None => false,
    }))
}

impl Drop for Busy {
    fn drop(&mut self) {
        if self.0 {
            owned(|o| {
                if let Some(own) = o {
                    own.busy = own.busy.saturating_sub(1);
                    own.last_used = Instant::now();
                }
            });
        }
    }
}

// ── Starting it ──────────────────────────────────────────────────────────────

/// Before talking to Unsloth Studio: started when the switch is on, it is
/// installed, a key is saved and it does not answer. Otherwise nothing is done,
/// and the usual "not answering" error follows.
pub fn ensure(app: &AppHandle, typed: &str, auto_start: bool) -> Result<(), String> {
    if !auto_start || keys::get(ID).is_none() {
        return Ok(());
    }
    let Some(base) = start_address(typed) else {
        return Ok(());
    };
    let Some(python) = python() else {
        return Ok(());
    };
    if any_answers(typed) {
        return Ok(());
    }
    let _one = STARTING.lock().unwrap_or_else(|e| e.into_inner());
    // Another caller's start may have finished meanwhile.
    if any_answers(typed) {
        return Ok(());
    }
    owned_ended();
    if owned(|o| o.is_none()) {
        let child = spawn(&python, base.port).map_err(|e| format!("ai.error.unslothStart|{e}"))?;
        owned(|o| {
            *o = Some(Owned {
                child,
                ready: false,
                last_used: Instant::now(),
                busy: 0,
            })
        });
    }
    let began = Instant::now();
    let mut said = 0;
    emit(app, "starting", 0);
    loop {
        std::thread::sleep(START_POLL);
        if owned_ended() {
            emit(app, "off", 0);
            return Err("ai.error.unslothStopped".into());
        }
        if answers(&base) {
            owned(|o| {
                if let Some(own) = o {
                    own.ready = true;
                    own.last_used = Instant::now();
                }
            });
            emit(app, "on", 0);
            watch_idle(app.clone());
            return Ok(());
        }
        let seconds = began.elapsed().as_secs();
        if seconds != said {
            said = seconds;
            emit(app, "starting", seconds);
        }
        if began.elapsed() > START_LIMIT {
            stop_owned();
            emit(app, "off", 0);
            return Err("ai.error.unslothSlow".into());
        }
    }
}

/// `unsloth studio` on `port`, on this PC only, with no window.
fn spawn(python: &Path, port: u16) -> std::io::Result<Child> {
    let mut cmd = Command::new(python);
    cmd.args(start_args(port))
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Its own folder, so what it writes beside itself stays there.
    if let Some(home) = python.ancestors().nth(3).filter(|d| d.is_dir()) {
        cmd.current_dir(home);
    }
    platform::no_console(&mut cmd);
    cmd.spawn()
}

fn start_args(port: u16) -> Vec<String> {
    let mut args: Vec<String> = ["-X", "utf8", "-c", ENTRY, "studio", "-H", "127.0.0.1", "-p"]
        .into_iter()
        .map(String::from)
        .collect();
    args.push(port.to_string());
    args.push("--silent".into());
    args
}

// ── Stopping it ──────────────────────────────────────────────────────────────

/// Whether the server the app started has had no question for `limit`.
fn idle_for(own: &Owned, limit: Duration) -> bool {
    own.ready && own.busy == 0 && own.last_used.elapsed() >= limit
}

/// While the app's Unsloth Studio runs: stopped once idle for `IDLE_LIMIT`.
fn watch_idle(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(IDLE_POLL);
        if owned_ended() {
            emit(&app, "off", 0);
            return;
        }
        match owned(|o| o.as_ref().map(|own| idle_for(own, IDLE_LIMIT))) {
            None => return,
            Some(true) => {
                stop_owned();
                emit(&app, "off", 0);
                return;
            }
            Some(false) => {}
        }
    });
}

/// Stops the Unsloth Studio the app started, with what it runs (its model
/// server). One the user opened is left alone.
pub fn stop_owned() {
    if let Some(mut own) = owned(Option::take) {
        platform::kill_tree(&mut own.child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_server_on_this_pc_over_http_is_started() {
        let port = |typed: &str| start_address(typed).map(|u| u.port);
        assert_eq!(port(""), Some(8888));
        assert_eq!(port("localhost:8000"), Some(8000));
        assert_eq!(port("http://127.0.0.1:9000/v1"), Some(9000));
        assert_eq!(port("https://127.0.0.1:8888"), None);
        assert_eq!(port("http://192.168.1.20:8888"), None);
    }

    #[test]
    fn it_starts_on_this_pc_only_and_quietly() {
        let args = start_args(8888);
        assert_eq!(
            args[4..],
            ["studio", "-H", "127.0.0.1", "-p", "8888", "--silent"]
        );
    }

    #[test]
    fn the_install_is_found_in_its_venv_or_the_older_one() {
        let dir = std::env::temp_dir().join(format!("eu-unsloth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(python_in(&dir), None);
        let old = dir.join(".venv").join("Scripts");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("python.exe"), "").unwrap();
        assert_eq!(python_in(&dir), Some(old.join("python.exe")));
        let new = dir.join("unsloth_studio").join("Scripts");
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("python.exe"), "").unwrap();
        assert_eq!(python_in(&dir), Some(new.join("python.exe")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
