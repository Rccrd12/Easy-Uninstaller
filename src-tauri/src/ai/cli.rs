//! Subscriptions through their own programs: Claude Code (`claude`), Codex
//! (`codex`) and Antigravity CLI (`agy`), installed and signed in by the user.
//!
//! The app runs the unmodified program, as the user would in a terminal, and
//! reads only its answer: it never reads, stores or forwards a sign-in.
//!
//! Every run is one question with nothing to act on:
//! - it starts in an empty folder of its own, emptied before each run;
//! - with its tools turned off or in its read-only / plan mode, and never with
//!   a flag that skips its permission checks;
//! - the question goes in on stdin, never on the command line (an npm install is
//!   a `.cmd`, and cmd.exe would read it as syntax); every argument is fixed or a
//!   checked model name;
//! - it is stopped, with everything it started, after a timeout.

use super::services::safe_model;
use super::AiModel;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const ASK_TIMEOUT: Duration = Duration::from_secs(240);
const MODELS_TIMEOUT: Duration = Duration::from_secs(25);
/// Output past this is not read.
const MAX_OUTPUT: u64 = 4 * 1024 * 1024;

/// Where `id`'s program may be, best first. Only those that exist.
pub fn candidates(id: &str) -> Vec<PathBuf> {
    let (stem, extra): (&str, Vec<PathBuf>) = match id {
        "claude-code" => (
            "claude",
            vec![
                var("USERPROFILE").map(|h| h.join(".local").join("bin").join("claude.exe")),
                var("APPDATA").map(|a| a.join("npm").join("claude.cmd")),
            ]
            .into_iter()
            .flatten()
            .collect(),
        ),
        "codex" => (
            "codex",
            vec![
                var("APPDATA").map(|a| a.join("npm").join("codex.cmd")),
                var("LOCALAPPDATA").map(|l| l.join("Volta").join("bin").join("codex.exe")),
                var("LOCALAPPDATA").map(|l| l.join("pnpm").join("codex.cmd")),
                var("USERPROFILE").map(|h| h.join(".bun").join("bin").join("codex.exe")),
                var("USERPROFILE").map(|h| h.join(".local").join("bin").join("codex.exe")),
            ]
            .into_iter()
            .flatten()
            .collect(),
        ),
        "antigravity-cli" => (
            "agy",
            vec![
                var("LOCALAPPDATA").map(|l| l.join("agy").join("bin").join("agy.exe")),
                var("USERPROFILE").map(|h| h.join(".local").join("bin").join("agy.exe")),
            ]
            .into_iter()
            .flatten()
            .collect(),
        ),
        _ => return Vec::new(),
    };
    let mut out: Vec<PathBuf> = platform::find_on_path(stem).into_iter().collect();
    out.extend(extra);
    out.retain(|p| p.is_file());
    out.dedup();
    out
}

fn var(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

fn exe(id: &str) -> Result<PathBuf, String> {
    candidates(id)
        .into_iter()
        .next()
        .ok_or_else(|| "ai.error.notInstalled".to_string())
}

/// An empty folder for one run, its own so several questions can run at once.
fn work_dir() -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir()
        .join("easy-uninstaller-ai")
        .join(format!("{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// The arguments for one question. `out` is where Codex writes its last message.
pub fn args(id: &str, model: &str, work: &Path, out: &Path) -> Vec<String> {
    let model = safe_model(model);
    let mut a: Vec<String> = match id {
        "claude-code" => [
            "-p",
            "--output-format",
            "json",
            // No tools at all, and plan mode in case a version ignores that.
            "--tools",
            "",
            "--permission-mode",
            "plan",
            "--strict-mcp-config",
            "--no-session-persistence",
        ]
        .map(String::from)
        .to_vec(),
        "codex" => vec![
            "exec".into(),
            "--sandbox".into(),
            "read-only".into(),
            "--skip-git-repo-check".into(),
            "--ephemeral".into(),
            "--output-last-message".into(),
            out.to_string_lossy().into_owned(),
        ],
        "antigravity-cli" => vec![
            "--input-format".into(),
            "stream-json".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--disable-slash-commands".into(),
            "--mode".into(),
            "plan".into(),
            "--print-timeout".into(),
            "4m".into(),
            "--add-dir".into(),
            work.to_string_lossy().into_owned(),
        ],
        _ => Vec::new(),
    };
    if let Some(m) = model {
        a.push(if id == "codex" { "-m" } else { "--model" }.into());
        a.push(m.into());
    }
    if id == "codex" {
        // The question comes on stdin.
        a.push("-".into());
    }
    a
}

/// What goes in on stdin.
pub fn stdin_text(id: &str, question: &str) -> String {
    match id {
        "antigravity-cli" => {
            json!({ "event": "user", "message": { "content": question } }).to_string() + "\n"
        }
        _ => question.to_string(),
    }
}

fn is_sign_in_error(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    [
        "login",
        "log in",
        "sign in",
        "signin",
        "not authenticated",
        "unauthorized",
        "401",
        "api key",
        "auth",
    ]
    .iter()
    .any(|w| t.contains(w))
}

fn short(text: &str) -> String {
    let t: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    t.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(240)
        .collect()
}

/// The answer in Claude Code's `--output-format json` output.
pub fn claude_answer(stdout: &str) -> Result<String, String> {
    let v = stdout
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .find(|v| v.get("result").is_some() || v["type"] == "result")
        .or_else(|| serde_json::from_str::<Value>(stdout.trim()).ok())
        .ok_or("ai.error.badAnswer")?;
    let text = v["result"].as_str().unwrap_or("").to_string();
    if v["is_error"].as_bool() == Some(true)
        || v["subtype"].as_str().is_some_and(|s| s != "success")
    {
        return Err(if is_sign_in_error(&text) {
            "ai.error.notSignedIn".into()
        } else {
            format!("ai.error.server|{}", short(&text))
        });
    }
    Ok(text)
}

/// The answer in Antigravity CLI's stream-json output: its `result` event.
pub fn agy_result(line: &str) -> Option<Result<String, String>> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if v["event"] != "result" {
        return None;
    }
    // Newer builds nest the result, older ones put it next to `event`.
    let r = if v["result"].is_object() {
        &v["result"]
    } else {
        &v
    };
    let status = r["status"].as_str().unwrap_or("").to_ascii_uppercase();
    let error = r["error"]
        .as_str()
        .map(str::to_string)
        .or_else(|| r["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_default();
    Some(
        if status == "SUCCESS" || (status.is_empty() && error.is_empty()) {
            Ok(r["response"].as_str().unwrap_or("").to_string())
        } else if is_sign_in_error(&error) {
            Err("ai.error.notSignedIn".into())
        } else {
            Err(format!("ai.error.server|{}", short(&error)))
        },
    )
}

/// Reads `pipe` in the background, at most MAX_OUTPUT, whole lines sent as they come.
fn read_lines(pipe: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(pipe.take(MAX_OUTPUT));
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn read_all(pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.take(MAX_OUTPUT).read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn spawn(exe: &Path, args: &[String], dir: &Path) -> Result<Child, String> {
    let mut cmd = Command::new(exe);
    // An npm install is a script that needs its own Node: its folder goes first.
    if let Some(parent) = exe.parent() {
        let mut dirs = vec![parent.to_path_buf()];
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path));
        }
        if let Ok(joined) = std::env::join_paths(dirs) {
            cmd.env("PATH", joined);
        }
    }
    cmd.args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    platform::no_console(&mut cmd);
    cmd.spawn().map_err(|e| format!("ai.error.server|{e}"))
}

/// Waits for `child` until `deadline`; past it, stops it and all it started.
fn wait(child: &mut Child, deadline: Instant) -> Result<(), String> {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                platform::kill_tree(child);
                return Err("ai.error.timeout".into());
            }
        }
    }
}

/// One question to `id`'s program, blocking.
pub fn ask(id: &str, model: &str, question: &str) -> Result<String, String> {
    let exe = exe(id)?;
    let work = work_dir()?;
    let out_file = work.join("answer.txt");
    let args = args(id, model, &work, &out_file);
    let mut child = spawn(&exe, &args, &work)?;
    let deadline = Instant::now() + ASK_TIMEOUT;
    let stderr = read_all(child.stderr.take().ok_or("no stderr")?);
    let mut stdin = child.stdin.take();
    if let Some(pipe) = stdin.as_mut() {
        let _ = pipe.write_all(stdin_text(id, question).as_bytes());
        let _ = pipe.flush();
    }

    let result = if id == "antigravity-cli" {
        // agy waits for more input: its stdin closes once the result came.
        let lines = read_lines(child.stdout.take().ok_or("no stdout")?);
        let mut found = None;
        while found.is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            match lines.recv_timeout(left) {
                Ok(line) => found = agy_result(&line),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    platform::kill_tree(&mut child);
                    return Err("ai.error.timeout".into());
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        drop(stdin);
        let _ = wait(&mut child, Instant::now() + Duration::from_secs(5));
        found.unwrap_or_else(|| Err(stderr_error(&stderr.join().unwrap_or_default())))
    } else {
        drop(stdin);
        let stdout = read_all(child.stdout.take().ok_or("no stdout")?);
        wait(&mut child, deadline)?;
        let stdout = stdout.join().unwrap_or_default();
        let ok = child.try_wait().ok().flatten().is_some_and(|s| s.success());
        if id == "claude-code" {
            match claude_answer(&stdout) {
                Err(e) if e == "ai.error.badAnswer" && !ok => {
                    Err(stderr_error(&stderr.join().unwrap_or_default()))
                }
                other => other,
            }
        } else {
            let answer = std::fs::read_to_string(&out_file).unwrap_or_default();
            if !answer.trim().is_empty() {
                Ok(answer)
            } else if ok && !stdout.trim().is_empty() {
                Ok(stdout)
            } else {
                Err(stderr_error(&stderr.join().unwrap_or_default()))
            }
        }
    };
    let _ = std::fs::remove_dir_all(&work);
    result
}

fn stderr_error(stderr: &str) -> String {
    if is_sign_in_error(stderr) {
        "ai.error.notSignedIn".into()
    } else if stderr.trim().is_empty() {
        "ai.error.badAnswer".into()
    } else {
        // The last lines say what went wrong.
        let tail: Vec<&str> = stderr
            .lines()
            .rev()
            .filter(|l| !l.trim().is_empty())
            .take(2)
            .collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        format!("ai.error.server|{}", short(&tail.join(" ")))
    }
}

/// Claude Code takes these names for its current models.
fn claude_models() -> Vec<AiModel> {
    ["sonnet", "opus", "haiku"]
        .iter()
        .map(|m| AiModel {
            id: m.to_string(),
            label: format!("{}{}", m[..1].to_ascii_uppercase(), &m[1..]),
        })
        .collect()
}

/// Models in a `model/list` answer of `codex app-server`.
pub fn codex_models(result: &Value) -> Vec<AiModel> {
    result["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["hidden"].as_bool() != Some(true))
        .filter_map(|m| {
            let id = m["model"]
                .as_str()
                .or_else(|| m["id"].as_str())
                .and_then(safe_model)?;
            let label = m["displayName"]
                .as_str()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or(id);
            Some(AiModel {
                id: id.into(),
                label: short(label),
            })
        })
        .take(50)
        .collect()
}

/// Models in `agy models` output, whatever its layout: JSON, one per line, or a
/// table whose first column is the model. Every model name has a digit in it.
pub fn agy_models(text: &str) -> Vec<AiModel> {
    let mut out: Vec<AiModel> = Vec::new();
    let mut add = |id: &str| {
        let id = id
            .trim()
            .trim_start_matches(['-', '*', '•', '›', '>'])
            .trim();
        let id = ["(current)", "(default)", "(selected)", "*"]
            .iter()
            .fold(id, |s, mark| s.strip_suffix(mark).unwrap_or(s).trim_end());
        if let Some(m) = safe_model(id).filter(|m| m.chars().any(|c| c.is_ascii_digit())) {
            if !out.iter().any(|o| o.id == m) && out.len() < 50 {
                out.push(AiModel {
                    id: m.into(),
                    label: m.into(),
                });
            }
        }
    };
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        let list = v
            .as_array()
            .or_else(|| v.get("models").and_then(Value::as_array));
        for item in list.into_iter().flatten() {
            let id = item.as_str().or_else(|| {
                ["id", "slug", "model", "name"]
                    .iter()
                    .find_map(|k| item.get(*k).and_then(Value::as_str))
            });
            if let Some(id) = id {
                add(id);
            }
        }
        return out;
    }
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.ends_with(':') {
            continue;
        }
        add(line
            .split('\t')
            .next()
            .unwrap_or("")
            .split("  ")
            .next()
            .unwrap_or(""));
    }
    out
}

/// The models `id`'s program offers. Empty when it cannot say.
pub fn models(id: &str) -> Result<Vec<AiModel>, String> {
    let exe = exe(id)?;
    if id == "claude-code" {
        return Ok(claude_models());
    }
    let work = work_dir()?;
    let (args, input): (Vec<String>, String) = if id == "codex" {
        let hello = json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "easy-uninstaller", "title": "Easy Uninstaller", "version": env!("CARGO_PKG_VERSION") } } });
        let ready = json!({ "method": "initialized" });
        let list = json!({ "id": 2, "method": "model/list", "params": {} });
        (
            vec!["app-server".into()],
            format!("{hello}\n{ready}\n{list}\n"),
        )
    } else {
        (vec!["models".into()], String::new())
    };
    let mut child = spawn(&exe, &args, &work)?;
    let mut stdin = child.stdin.take();
    if let Some(pipe) = stdin.as_mut() {
        let _ = pipe.write_all(input.as_bytes());
        let _ = pipe.flush();
    }
    if id != "codex" {
        drop(stdin.take());
    }
    let lines = read_lines(child.stdout.take().ok_or("no stdout")?);
    let deadline = Instant::now() + MODELS_TIMEOUT;
    let mut text = String::new();
    let mut found = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match lines.recv_timeout(left) {
            Ok(line) if id == "codex" => {
                let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                if v["id"] == 2 {
                    found = codex_models(&v["result"]);
                    break;
                }
            }
            Ok(line) => {
                text.push_str(&line);
                text.push('\n');
            }
            Err(_) => break,
        }
    }
    drop(stdin);
    platform::kill_tree(&mut child);
    let _ = std::fs::remove_dir_all(&work);
    if id == "antigravity-cli" {
        found = agy_models(&text);
    }
    Ok(found)
}

#[cfg(windows)]
pub(super) mod platform {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::{Child, Command};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn no_console(cmd: &mut Command) {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    /// An npm install runs a `.cmd`: stopping cmd.exe alone would leave node running.
    pub fn kill_tree(child: &mut Child) {
        let mut kill = Command::new("taskkill");
        kill.args(["/T", "/F", "/PID", &child.id().to_string()]);
        no_console(&mut kill);
        let _ = kill.status();
        let _ = child.kill();
        let _ = child.wait();
    }

    /// `stem` with one of PATHEXT's extensions, in a folder of PATH.
    pub fn find_on_path(stem: &str) -> Option<PathBuf> {
        let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        let dirs = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&dirs) {
            if !dir.is_absolute() {
                continue;
            }
            for ext in exts.split(';').filter(|e| !e.is_empty()) {
                let candidate = dir.join(format!("{stem}{}", ext.to_lowercase()));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }
}

#[cfg(not(windows))]
pub(super) mod platform {
    use std::path::PathBuf;
    use std::process::{Child, Command};

    pub fn no_console(_cmd: &mut Command) {}

    pub fn kill_tree(child: &mut Child) {
        let _ = child.kill();
        let _ = child.wait();
    }

    pub fn find_on_path(stem: &str) -> Option<PathBuf> {
        let dirs = std::env::var_os("PATH")?;
        std::env::split_paths(&dirs)
            .filter(|d| d.is_absolute())
            .map(|d| d.join(stem))
            .find(|p| p.is_file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_run_without_tools_or_skipped_checks() {
        let work = Path::new("/tmp/w");
        let out = Path::new("/tmp/w/answer.txt");
        for id in ["claude-code", "codex", "antigravity-cli"] {
            let a = args(id, "", work, out);
            assert!(!a.is_empty());
            assert!(
                !a.iter()
                    .any(|s| s.contains("dangerously") || s.contains("bypass") || s == "--yolo"),
                "{id}: {a:?}"
            );
            assert!(
                !a.iter().any(|s| s == "--model" || s == "-m"),
                "no model, the program's own"
            );
        }
        let c = args("claude-code", "sonnet", work, out);
        assert!(c.windows(2).any(|w| w == ["--tools", ""]));
        assert!(c.windows(2).any(|w| w == ["--permission-mode", "plan"]));
        assert!(c.windows(2).any(|w| w == ["--model", "sonnet"]));
        let x = args("codex", "gpt-5.5", work, out);
        assert!(x.windows(2).any(|w| w == ["--sandbox", "read-only"]));
        assert!(x.windows(2).any(|w| w == ["-m", "gpt-5.5"]));
        assert_eq!(x.last().unwrap(), "-");
        let g = args("antigravity-cli", "Gemini 3.1 Pro (High)", work, out);
        assert!(g.windows(2).any(|w| w == ["--mode", "plan"]));
        assert!(
            !g.iter().any(|s| s == "--model"),
            "an unsafe model name is left out"
        );
        assert!(!g.iter().any(|s| s == "-p"));
        // The question is never an argument.
        assert!(stdin_text("antigravity-cli", "Folder: X").contains("\"content\":\"Folder: X\""));
        assert_eq!(stdin_text("claude-code", "Folder: X"), "Folder: X");
    }

    #[test]
    fn answers_and_errors_are_read() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":"{\"verdict\":\"keep\"}"}"#;
        assert_eq!(claude_answer(ok).unwrap(), "{\"verdict\":\"keep\"}");
        let login = r#"{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login"}"#;
        assert_eq!(claude_answer(login).unwrap_err(), "ai.error.notSignedIn");
        assert_eq!(claude_answer("garbage").unwrap_err(), "ai.error.badAnswer");

        assert_eq!(agy_result(r#"{"event":"step_update"}"#), None);
        let done = r#"{"event":"result","result":{"status":"SUCCESS","response":"hi"}}"#;
        assert_eq!(agy_result(done), Some(Ok("hi".into())));
        let old = r#"{"event":"result","status":"SUCCESS","response":"hi"}"#;
        assert_eq!(agy_result(old), Some(Ok("hi".into())));
        let err = r#"{"event":"result","result":{"status":"ERROR","error":"quota exceeded"}}"#;
        assert_eq!(
            agy_result(err),
            Some(Err("ai.error.server|quota exceeded".into()))
        );

        assert_eq!(
            stderr_error("Error: Not logged in. Run codex login"),
            "ai.error.notSignedIn"
        );
        assert_eq!(stderr_error(""), "ai.error.badAnswer");
    }

    #[test]
    fn model_lists_are_read() {
        let codex = json!({ "data": [
            { "model": "gpt-5.5", "displayName": "GPT-5.5" },
            { "model": "old", "hidden": true },
            { "id": "bad name" }
        ]});
        let m = codex_models(&codex);
        assert_eq!(m.len(), 1);
        assert_eq!(
            (m[0].id.as_str(), m[0].label.as_str()),
            ("gpt-5.5", "GPT-5.5")
        );

        let table = "Available models:\n- gemini-3.1-pro-high (current)\n- gemini-3.8-flash-low\nTip: use --model\n";
        let ids: Vec<_> = agy_models(table).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, ["gemini-3.1-pro-high", "gemini-3.8-flash-low"]);
        let json = r#"{"models":[{"id":"gemini-3-pro"},"gemini-3-flash"]}"#;
        assert_eq!(agy_models(json).len(), 2);
    }
}
