//! llama.cpp's server, started by the app with the downloaded model when the
//! index or a search needs it, on this PC only (127.0.0.1, a free port), with no
//! window and at low priority so the PC stays usable. It stops after a few idle
//! minutes and when the app closes.

use super::store;
use crate::ai::http::{self, HttpError, Url};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Loading the model takes a few seconds; a slow disk or PC, longer.
const START_LIMIT: Duration = Duration::from_secs(180);
const IDLE_LIMIT: Duration = Duration::from_secs(4 * 60);
/// One picture on a slow PC: generous, the server answers one request at a time.
const REQUEST_LIMIT: Duration = Duration::from_secs(300);

/// What the server is started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    pub exe: PathBuf,
    pub text: PathBuf,
    pub vision: PathBuf,
    pub log: PathBuf,
}

struct Running {
    child: Child,
    port: u16,
    setup: Setup,
    last_used: Instant,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// What to turn into a vector.
pub enum Input<'a> {
    /// A search: what the person wants to find.
    Query(&'a str),
    /// A picture (PNG): a photo, or a frame of a video.
    Picture(&'a [u8]),
}

/// The request body for llama.cpp's own `/embeddings` endpoint. Pictures go as
/// base64 with the media marker in the prompt; EmbeddingGemma wants no prefix for
/// them, and `task: search result | query: ` before a search.
pub fn body(input: &Input) -> Value {
    match input {
        Input::Query(text) => json!({ "content": format!("task: search result | query: {text}") }),
        Input::Picture(png) => json!({
            "content": [{ "prompt_string": "<__media__>", "multimodal_data": [crate::apps::icon::base64(png)] }]
        }),
    }
}

/// The vector in llama.cpp's answer: `[{"embedding": [[...]]}]`, or one level
/// less. One vector per token (no pooling) is averaged.
pub fn vector_of(answer: &Value) -> Option<Vec<f32>> {
    fn numbers(v: &Value) -> Option<Vec<f32>> {
        let a = v.as_array()?;
        if a.is_empty() || !a.iter().all(Value::is_number) {
            return None;
        }
        Some(
            a.iter()
                .filter_map(Value::as_f64)
                .map(|x| x as f32)
                .collect(),
        )
    }
    let first = match answer {
        Value::Array(items) => items.first()?,
        other => other,
    };
    let emb = first
        .get("embedding")
        .or_else(|| first.pointer("/data/0/embedding"))?;
    if let Some(v) = numbers(emb) {
        return Some(v);
    }
    let rows: Vec<Vec<f32>> = emb.as_array()?.iter().filter_map(numbers).collect();
    let len = rows.first()?.len();
    if rows.iter().any(|r| r.len() != len) {
        return None;
    }
    let mut mean = vec![0f32; len];
    for r in &rows {
        for (m, x) in mean.iter_mut().zip(r) {
            *m += x / rows.len() as f32;
        }
    }
    Some(mean)
}

/// The last lines llama.cpp wrote, to say why it stopped.
fn log_tail(log: &Path) -> String {
    let Ok(mut f) = File::open(log) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(600)));
    let mut text = String::new();
    let _ = f.read_to_string(&mut text);
    text.lines()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" ")
}

fn free_port() -> Result<u16, String> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

fn url(port: u16, path: &str) -> Url {
    Url {
        https: false,
        host: "127.0.0.1".into(),
        port,
        path: path.into(),
    }
}

fn threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).max(1))
        .unwrap_or(2)
}

fn start(setup: &Setup) -> Result<Running, String> {
    let port = free_port()?;
    let log = File::create(&setup.log).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(&setup.exe);
    cmd.arg("--model")
        .arg(&setup.text)
        .arg("--mmproj")
        .arg(&setup.vision)
        .args(["--embeddings", "--pooling", "mean"])
        .args([
            "--ctx-size",
            "4096",
            "--batch-size",
            "4096",
            "--ubatch-size",
            "4096",
        ])
        .args(["--parallel", "1", "--host", "127.0.0.1", "--port"])
        .arg(port.to_string())
        .arg("--threads")
        .arg(threads().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    if let Some(dir) = setup.exe.parent() {
        cmd.current_dir(dir);
    }
    platform::quiet(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("index.error.engine:{e}"))?;
    let deadline = Instant::now() + START_LIMIT;
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            return Err(format!("index.error.engine:{}", log_tail(&setup.log)));
        }
        match http::request(
            "GET",
            &url(port, "/health"),
            &[],
            None,
            Duration::from_secs(2),
        ) {
            Ok(r) if r.status == 200 => break,
            _ if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("index.error.engineSlow".into());
            }
            _ => std::thread::sleep(Duration::from_millis(400)),
        }
    }
    Ok(Running {
        child,
        port,
        setup: setup.clone(),
        last_used: Instant::now(),
    })
}

/// One vector, unit length. Starts the server when it is not running (or runs
/// another model). Blocking; one request at a time.
pub fn embed(setup: &Setup, input: &Input) -> Result<Vec<f32>, String> {
    let mut running = RUNNING.lock().map_err(|e| e.to_string())?;
    let stale = match running.as_mut() {
        Some(r) => r.setup != *setup || matches!(r.child.try_wait(), Ok(Some(_))),
        None => true,
    };
    if stale {
        if let Some(mut old) = running.take() {
            let _ = old.child.kill();
            let _ = old.child.wait();
        }
        *running = Some(start(setup)?);
        watch_idle();
    }
    let r = running.as_mut().ok_or("index.error.engineSlow")?;
    r.last_used = Instant::now();
    let body = serde_json::to_vec(&body(input)).map_err(|e| e.to_string())?;
    let answer = http::request(
        "POST",
        &url(r.port, "/embeddings"),
        &[],
        Some(&body),
        REQUEST_LIMIT,
    );
    r.last_used = Instant::now();
    let answer = match answer {
        Ok(a) => a,
        Err(HttpError::Timeout) => return Err("index.error.engineSlow".into()),
        Err(e) => {
            // It may have stopped: the next request starts it again.
            if let Some(mut gone) = running.take() {
                let _ = gone.child.kill();
                let _ = gone.child.wait();
            }
            return Err(format!("index.error.engine:{e:?}"));
        }
    };
    if answer.status != 200 {
        let text = String::from_utf8_lossy(&answer.body);
        return Err(format!(
            "index.error.engine:{} {}",
            answer.status,
            crate::ai::prompt::tidy(&text, 200)
        ));
    }
    let value: Value =
        serde_json::from_slice(&answer.body).map_err(|_| "index.error.engine:answer")?;
    let v = vector_of(&value).ok_or("index.error.engine:answer")?;
    store::unit(&v).ok_or_else(|| "index.error.engine:answer".into())
}

/// Stops the server after a few minutes without requests. One watcher per start.
fn watch_idle() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(20));
        let Ok(mut running) = RUNNING.lock() else {
            return;
        };
        match running.as_mut() {
            None => return,
            Some(r) if r.last_used.elapsed() > IDLE_LIMIT => {
                let _ = r.child.kill();
                let _ = r.child.wait();
                *running = None;
                return;
            }
            Some(_) => {}
        }
    });
}

/// Stops the server now (the app closes, or the model is removed).
pub fn stop() {
    if let Ok(mut running) = RUNNING.lock() {
        if let Some(mut r) = running.take() {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;

    pub fn quiet(cmd: &mut Command) {
        cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn quiet(_cmd: &mut std::process::Command) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_follow_embeddinggemma_and_llama_cpp() {
        let q = body(&Input::Query("vacanze al mare"));
        assert_eq!(q["content"], "task: search result | query: vacanze al mare");
        let p = body(&Input::Picture(b"png"));
        assert_eq!(p["content"][0]["prompt_string"], "<__media__>");
        assert_eq!(p["content"][0]["multimodal_data"][0], "cG5n");
    }

    #[test]
    fn answers_are_read_in_every_shape() {
        let pooled = json!([{ "index": 0, "embedding": [[0.5, 1.0]] }]);
        assert_eq!(vector_of(&pooled), Some(vec![0.5, 1.0]));
        let flat = json!([{ "index": 0, "embedding": [0.5, 1.0] }]);
        assert_eq!(vector_of(&flat), Some(vec![0.5, 1.0]));
        let tokens = json!([{ "embedding": [[1.0, 0.0], [0.0, 1.0]] }]);
        assert_eq!(vector_of(&tokens), Some(vec![0.5, 0.5]));
        let oai = json!({ "data": [{ "embedding": [2.0, 3.0] }] });
        assert_eq!(vector_of(&oai), Some(vec![2.0, 3.0]));
        assert_eq!(vector_of(&json!({ "error": "no" })), None);
        assert_eq!(
            vector_of(&json!([{ "embedding": [[1.0], [1.0, 2.0]] }])),
            None
        );
    }
}
