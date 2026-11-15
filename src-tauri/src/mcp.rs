//! MCP server: other programs (Claude Desktop, Claude Code, a script) read what the
//! app finds, through the Model Context Protocol.
//!
//! The client starts this app as `easy-uninstaller.exe --mcp` and talks JSON-RPC 2.0
//! over stdin and stdout, one message per line: no window and no network port.
//! Every tool is read only: nothing is removed, moved or turned off, and nothing is
//! sent anywhere but back to the client that asked. The server answers only while
//! "Allow MCP clients" is on in Settings (`mcp.json`, off by default); the last client
//! is noted in `mcp-client.json` so that Settings can show it.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// The command-line flag the client starts the app with.
pub const FLAG: &str = "--mcp";
const SETTINGS_FILE: &str = "mcp.json";
const CLIENT_FILE: &str = "mcp-client.json";
/// Newest first: the first is answered when the client asks for one not listed.
const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const OFF: &str = "The MCP server is off. Turn on \"Allow MCP clients\" in Easy Uninstaller, Settings > MCP server.";
const INSTRUCTIONS: &str = "Easy Uninstaller reads this Windows PC: installed apps, startup programs, \
app caches, files that can be cleaned up, leftovers of uninstalled programs, where disk space goes, \
the Quarantine and the history of what the app removed. Every tool is read only. Sizes are in bytes, \
times in Unix seconds unless a field says otherwise. scan_files and scan_space_map walk the disk and \
can take a minute or more.";
/// The English UI texts: file group titles, leftover reasons and folder explanations
/// reach Rust as i18n keys, and a client reads them better as words.
const ENGLISH: &str = include_str!("../../src/lib/i18n/en.json");
/// At most one progress notification this often, so a fast scan does not flood the client.
const PROGRESS_EVERY: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// Settings, shared with the window.

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    enabled: bool,
}

/// Mirrors `McpClient` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpClient {
    pub name: String,
    pub last_seen: u64,
    pub calls: u64,
}

/// Mirrors `McpStatus` in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    /// This app's full path, for the client's configuration.
    pub command: String,
    pub last_client: Option<McpClient>,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Writes through a temporary file, so a reader never sees half a file.
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn is_enabled(dir: &Path) -> bool {
    read_json::<Settings>(&dir.join(SETTINGS_FILE)).is_some_and(|s| s.enabled)
}

fn status(dir: &Path) -> McpStatus {
    McpStatus {
        enabled: is_enabled(dir),
        command: std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        last_client: read_json(&dir.join(CLIENT_FILE)),
    }
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_local_data_dir().map_err(|e| e.to_string())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[tauri::command]
pub fn mcp_status(app: AppHandle) -> Result<McpStatus, String> {
    Ok(status(&data_dir(&app)?))
}

/// Off takes effect at the client's next request: a running server checks before each one.
#[tauri::command]
pub fn mcp_set_enabled(app: AppHandle, enabled: bool) -> Result<McpStatus, String> {
    let dir = data_dir(&app)?;
    write_json(&dir.join(SETTINGS_FILE), &Settings { enabled })?;
    Ok(status(&dir))
}

// ---------------------------------------------------------------------------
// The protocol: JSON-RPC 2.0, one message per line.

/// What the server can do, replaced by a fake in the tests.
pub trait Tools: Sync {
    /// Read again before each request, so turning the server off works at once.
    fn enabled(&self) -> bool;
    /// The tool definitions for `tools/list`.
    fn list(&self) -> Value;
    /// Runs a tool. `progress` takes a short message while a long scan runs.
    fn call(&self, name: &str, args: &Value, progress: &dyn Fn(&str)) -> Result<Value, CallError>;
    /// Notes the client and how many tools it called, for Settings.
    fn seen(&self, client: &str, calls: u64);
}

#[derive(Debug, PartialEq, Eq)]
pub enum CallError {
    /// No tool with that name: a protocol error.
    Unknown,
    /// The tool ran and failed: told to the client as a tool result.
    Failed(String),
}

struct Session<'a, T: Tools, W: Write> {
    tools: &'a T,
    out: Mutex<W>,
    client: Mutex<String>,
    calls: AtomicU64,
}

impl<T: Tools, W: Write + Send> Session<'_, T, W> {
    fn send(&self, message: &Value) {
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        // The client is gone if this fails: the read loop ends on its own.
        let _ = writeln!(out, "{message}");
        let _ = out.flush();
    }

    fn reply(&self, id: &Value, result: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    fn fail(&self, id: &Value, code: i64, message: &str) {
        self.send(
            &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
        );
    }

    fn client(&self) -> String {
        self.client
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn handle(&self, message: &Value) {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return; // A response to a request this server never sends.
        };
        // Notifications (initialized, cancelled) need no answer.
        let Some(id) = message.get("id") else {
            return;
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "initialize" => {
                if let Some(name) = params.pointer("/clientInfo/name").and_then(Value::as_str) {
                    *self.client.lock().unwrap_or_else(|e| e.into_inner()) = name.to_string();
                }
                if !self.tools.enabled() {
                    return self.fail(id, -32000, OFF);
                }
                self.tools.seen(&self.client(), 0);
                let asked = params.get("protocolVersion").and_then(Value::as_str);
                let version = asked
                    .filter(|v| PROTOCOL_VERSIONS.contains(v))
                    .unwrap_or(PROTOCOL_VERSIONS[0]);
                self.reply(
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": { "tools": {} },
                        "serverInfo": {
                            "name": "easy-uninstaller",
                            "title": "Easy Uninstaller",
                            "version": env!("CARGO_PKG_VERSION"),
                        },
                        "instructions": INSTRUCTIONS,
                    }),
                );
            }
            "ping" => self.reply(id, json!({})),
            "tools/list" => self.reply(id, json!({ "tools": self.tools.list() })),
            "tools/call" => self.call(id, &params),
            _ => self.fail(id, -32601, &format!("Method not found: {method}")),
        }
    }

    fn call(&self, id: &Value, params: &Value) {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = match params.get("arguments") {
            None | Some(Value::Null) => json!({}),
            Some(a) => a.clone(),
        };
        if !self.tools.enabled() {
            return self.reply(id, tool_error(OFF));
        }
        let calls = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        self.tools.seen(&self.client(), calls);
        let token = params.pointer("/_meta/progressToken").cloned();
        // (notifications sent, time of the last one)
        let sent = Mutex::new((0u64, None::<Instant>));
        let progress = |text: &str| {
            let Some(token) = &token else { return };
            let mut sent = sent.lock().unwrap_or_else(|e| e.into_inner());
            if sent.1.is_some_and(|t| t.elapsed() < PROGRESS_EVERY) {
                return;
            }
            // The progress value must grow with each notification; the total is unknown.
            *sent = (sent.0 + 1, Some(Instant::now()));
            self.send(&json!({
                "jsonrpc": "2.0",
                "method": "notifications/progress",
                "params": { "progressToken": token, "progress": sent.0, "message": text },
            }));
        };
        match self.tools.call(name, &args, &progress) {
            Ok(value) => self.reply(
                id,
                json!({ "content": [{ "type": "text", "text": value.to_string() }] }),
            ),
            Err(CallError::Failed(e)) => self.reply(id, tool_error(&e)),
            Err(CallError::Unknown) => self.fail(id, -32602, &format!("Unknown tool: {name}")),
        }
    }
}

fn tool_error(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

/// Answers requests from `input` until the client closes it. Tool calls run on their
/// own threads, so a long scan does not hold up a ping or another tool.
pub fn serve(tools: &impl Tools, input: impl BufRead, out: impl Write + Send) {
    let session = Session {
        tools,
        out: Mutex::new(out),
        client: Mutex::new(String::new()),
        calls: AtomicU64::new(0),
    };
    std::thread::scope(|scope| {
        for line in input.lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let message: Value = match serde_json::from_str(&line) {
                Ok(m @ Value::Object(_)) => m,
                Ok(_) => {
                    session.fail(&Value::Null, -32600, "Invalid request");
                    continue;
                }
                Err(_) => {
                    session.fail(&Value::Null, -32700, "Parse error");
                    continue;
                }
            };
            let session = &session;
            if message.get("method").and_then(Value::as_str) == Some("tools/call") {
                scope.spawn(move || session.handle(&message));
            } else {
                session.handle(&message);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// The tools.

fn tool(name: &str, title: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
        "annotations": { "readOnlyHint": true, "destructiveHint": false, "openWorldHint": false },
    })
}

fn count(description: &str, default: u64, max: u64) -> Value {
    json!({ "type": "integer", "minimum": 0, "maximum": max, "default": default, "description": description })
}

/// The tool definitions, in the order a client lists them.
pub fn definitions() -> Value {
    let none = json!({});
    Value::Array(vec![
        tool("get_disk_space", "Disk space",
            "Free and total space of the Windows drive, and of the other fixed drives.",
            none.clone(), &[]),
        tool("list_installed_apps", "Installed apps",
            "Apps installed on this PC (desktop programs and Microsoft Store apps), biggest first: name, publisher, \
             size, version, install date, last use when known, whether it came with the PC and whether it is usually \
             unneeded (bloatware). cacheId matches list_app_caches. Takes a few seconds.",
            none.clone(), &[]),
        tool("list_startup_apps", "Startup apps",
            "Programs that start with Windows: on or off, impact on startup time (high, medium, low) and the app's \
             advice (turnOff, keep, system: part of Windows).",
            none.clone(), &[]),
        tool("list_app_caches", "App caches",
            "Caches of well-known apps (Teams, Spotify, Discord, browsers...) on this PC: what emptying the cache \
             would free, the app's other data, and whether the app is open. Ids match cacheId in list_installed_apps.",
            none.clone(), &[]),
        tool("scan_files", "Scan files",
            "Scans the PC for space that can be freed and returns the groups (temporary files, Recycle Bin, crash \
             reports, thumbnails, old installers, large files, duplicates, similar photos, big app folders...) with \
             size, file count, safety (safe, check, personal) and the largest files of each. Takes one minute or more \
             and sends progress. get_file_group lists every file of a group afterwards.",
            json!({ "maxItems": count("Largest files listed per group.", 10, 200) }), &[]),
        tool("get_file_group", "Files of a group",
            "Every file of one group of the last scan_files in this session, largest first; for duplicates and \
             similar photos, the sets (the first file of each set is the one kept by default).",
            json!({
                "groupId": { "type": "string", "description": "A group id from scan_files, e.g. \"large-files\"." },
                "limit": count("Files (or sets) listed.", 100, 2000),
            }), &["groupId"]),
        tool("list_leftovers", "Leftovers",
            "What uninstalled programs left behind: folders, registry keys, broken shortcuts and app-list entries, \
             with size, reason, and safety (check: the user's own settings of the app).",
            none.clone(), &[]),
        tool("scan_space_map", "Space map",
            "Where the space on the Windows drive goes: the biggest folders, each in a category (apps, personal, \
             reclaimable). Takes up to a minute and sends progress.",
            none.clone(), &[]),
        tool("scan_offload", "OneDrive and other drives",
            "Files in OneDrive that could be made online only, large files that could move to another drive, and \
             files moved there before.",
            json!({ "maxItems": count("Largest files listed in each list.", 20, 200) }), &[]),
        tool("list_quarantine", "Quarantine",
            "What is in Easy Uninstaller's Quarantine: removed items kept for a few days so they can be restored, \
             with removal and expiry times and sizes.",
            json!({ "maxItems": count("Items listed per entry.", 10, 50) }), &[]),
        tool("get_history", "History",
            "What Easy Uninstaller did on this PC (cleanups, uninstalls, restores) and how much space it gave back.",
            none.clone(), &[]),
        tool("list_uninstalled_apps", "Uninstalled apps",
            "Apps uninstalled with Easy Uninstaller, newest first, with the way to get each one back (Store, \
             website, Steam, web search).",
            none.clone(), &[]),
        tool("search_files", "Search files",
            "Searches the user's folders by kind, size, age and words in the name. Returns the first matches in the \
             chosen order and the totals.",
            json!({
                "kinds": { "type": "array", "items": { "type": "string", "enum": ["image", "video", "audio", "document", "archive", "program", "disc"] }, "description": "Any of these kinds; empty for any file." },
                "minBytes": { "type": "integer", "minimum": 0 },
                "maxBytes": { "type": "integer", "minimum": 0 },
                "unusedDays": { "type": "integer", "minimum": 0, "description": "Not opened or changed for at least this many days." },
                "recentDays": { "type": "integer", "minimum": 0, "description": "Changed in the last this many days." },
                "places": { "type": "array", "items": { "type": "string", "enum": ["desktop", "documents", "downloads", "pictures", "videos", "music"] }, "description": "Empty for all the user's folders." },
                "words": { "type": "array", "items": { "type": "string" }, "description": "Words that must all be in the file or folder names." },
                "sort": { "type": "string", "enum": ["size", "oldest", "newest"], "default": "size" },
                "limit": count("Files listed.", 50, 200),
            }), &[]),
        tool("get_downloads_overview", "Downloads",
            "How many loose files the Downloads folder has and their size, and the last tidy-up that can still be undone.",
            none, &[]),
        tool("explain_folder", "Explain a folder",
            "What a Windows or app folder is and whether it can be removed, from the app's built-in list (no AI). \
             known is false for folders not in the list.",
            json!({ "path": { "type": "string", "description": "A full folder path." } }), &["path"]),
    ])
}

/// The tools of the real app.
struct AppTools {
    app: AppHandle,
    dir: PathBuf,
}

fn arg_count(args: &Value, name: &str, default: u64, max: u64) -> usize {
    args.get(name)
        .and_then(Value::as_u64)
        .unwrap_or(default)
        .min(max) as usize
}

fn to_value(value: impl Serialize) -> Result<Value, CallError> {
    serde_json::to_value(value).map_err(|e| CallError::Failed(e.to_string()))
}

fn failed(e: impl ToString) -> CallError {
    CallError::Failed(e.to_string())
}

/// Runs a scan that reports progress on a channel, as the window would get it, and
/// passes each update on to `progress` as a short text.
fn with_progress<T: Send, R: Send>(
    progress: &dyn Fn(&str),
    text: impl Fn(&Value) -> String + Send + Sync + 'static,
    run: impl FnOnce(tauri::ipc::Channel<T>) -> R + Send,
) -> R {
    let (tx, updates) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    let channel = tauri::ipc::Channel::new(move |body| {
        if let tauri::ipc::InvokeResponseBody::Json(json) = body {
            if let Ok(update) = serde_json::from_str::<Value>(&json) {
                let _ = tx
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .send(text(&update));
            }
        }
        Ok(())
    });
    std::thread::scope(|scope| {
        let job = scope.spawn(move || run(channel));
        // Ends when the job is done and drops the channel, the only sender.
        for update in updates {
            progress(&update);
        }
        job.join().unwrap_or_else(|e| std::panic::resume_unwind(e))
    })
}

impl Tools for AppTools {
    fn enabled(&self) -> bool {
        is_enabled(&self.dir)
    }

    fn list(&self) -> Value {
        definitions()
    }

    fn seen(&self, client: &str, calls: u64) {
        let client = McpClient {
            name: if client.is_empty() {
                "MCP".into()
            } else {
                client.into()
            },
            last_seen: now_secs(),
            calls,
        };
        // Only for Settings: a failure here never fails the request.
        let _ = write_json(&self.dir.join(CLIENT_FILE), &client);
    }

    fn call(&self, name: &str, args: &Value, progress: &dyn Fn(&str)) -> Result<Value, CallError> {
        use tauri::async_runtime::block_on;
        let app = self.app.clone();
        let value = match name {
            "get_disk_space" => json!({
                "system": to_value(crate::disk::get_system_disk().map_err(failed)?)?,
                "otherDrives": to_value(crate::disk::other_drives())?,
            }),
            "list_installed_apps" => {
                to_value(block_on(crate::apps::list_installed_apps()).map_err(failed)?)?
            }
            "list_startup_apps" => {
                let mut v =
                    to_value(block_on(crate::startup::list_startup_apps()).map_err(failed)?)?;
                prefix_keys(&mut v, "note", "startup.notes.");
                v
            }
            "list_app_caches" => {
                to_value(block_on(crate::app_cache::list_app_caches(app)).map_err(failed)?)?
            }
            "scan_files" => {
                let max = arg_count(args, "maxItems", 10, 200);
                let groups = with_progress(
                    progress,
                    |p| {
                        format!(
                            "Step {} of {} ({}): {} files checked",
                            p["stepIndex"],
                            p["stepCount"],
                            p["step"].as_str().unwrap_or(""),
                            p["filesChecked"]
                        )
                    },
                    |channel| block_on(crate::files::scan_files(app, channel)),
                )
                .map_err(failed)?;
                let mut v = to_value(groups)?;
                for group in v.as_array_mut().into_iter().flatten() {
                    truncate(group, "items", max);
                }
                v
            }
            "get_file_group" => {
                let id = args
                    .get("groupId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let limit = arg_count(args, "limit", 100, 2000);
                let review =
                    block_on(crate::review::review_file_group(app, id.clone())).map_err(failed)?;
                let Some(review) = review else {
                    return Err(CallError::Failed(format!(
                        "No group \"{id}\" in this session: call scan_files first, and use one of its group ids."
                    )));
                };
                let mut v = to_value(review)?;
                truncate(&mut v, "files", limit);
                truncate(&mut v, "duplicateSets", limit);
                v
            }
            "list_leftovers" => {
                let mut v =
                    to_value(block_on(crate::leftovers::list_leftovers(app)).map_err(failed)?)?;
                prefix_keys(&mut v, "reason", "leftovers.reasons.");
                v
            }
            "scan_space_map" => to_value(
                with_progress(
                    progress,
                    |p| format!("{} files checked", p["filesChecked"]),
                    |channel| block_on(crate::space::scan_space(channel)),
                )
                .map_err(failed)?,
            )?,
            "scan_offload" => {
                let max = arg_count(args, "maxItems", 20, 200);
                let mut v = to_value(block_on(crate::offload::scan_offload(app)).map_err(failed)?)?;
                for part in ["oneDrive", "moveFiles"] {
                    if let Some(p) = v.get_mut(part) {
                        truncate(p, "files", max);
                    }
                }
                v
            }
            "list_quarantine" => {
                let max = arg_count(args, "maxItems", 10, 50);
                // Listed as they are: expired entries are deleted by the app itself, not here.
                let mut v = to_value(self.app.state::<crate::quarantine::Quarantine>().list())?;
                for entry in v.as_array_mut().into_iter().flatten() {
                    truncate(entry, "items", max);
                }
                v
            }
            "get_history" => {
                let entries = self.app.state::<crate::quarantine::Quarantine>().list();
                let live = crate::history::live_entries(&entries);
                to_value(self.app.state::<crate::history::History>().summary(&live))?
            }
            "list_uninstalled_apps" => to_value(crate::reinstall::list_uninstalled_apps(app))?,
            "search_files" => {
                let limit = arg_count(args, "limit", 50, 200);
                let mut filter: crate::search::SearchFilter = serde_json::from_value(args.clone())
                    .map_err(|e| CallError::Failed(format!("Invalid filter: {e}")))?;
                // Searching by content needs the local model, which only the window runs.
                filter.topic = None;
                let mut v =
                    to_value(block_on(crate::search::search_files(app, filter)).map_err(failed)?)?;
                truncate(&mut v, "files", limit);
                v
            }
            "get_downloads_overview" => {
                to_value(block_on(crate::tidy::tidy_overview(app)).map_err(failed)?)?
            }
            "explain_folder" => {
                let path = args.get("path").and_then(Value::as_str).unwrap_or("");
                if path.is_empty() {
                    return Err(CallError::Failed("path is required.".into()));
                }
                let known = crate::explain::KnownFolders::from_env();
                explain(known.lookup(Path::new(path)))
            }
            _ => return Err(CallError::Unknown),
        };
        Ok(readable(value))
    }
}

fn explain(folder: Option<&crate::explain::KnownFolder>) -> Value {
    let Some(folder) = folder else {
        return json!({ "known": false });
    };
    let mut v = json!({ "known": true, "id": folder.id, "verdict": folder.verdict });
    if let Some(Value::Object(texts)) =
        english().pointer(&format!("/explain/folders/{}", folder.id))
    {
        for (k, t) in texts {
            v[k] = t.clone();
        }
    }
    v
}

// ---------------------------------------------------------------------------
// Shaping the answers: words instead of i18n keys, no icons, short lists.

fn english() -> &'static Value {
    static TEXTS: OnceLock<Value> = OnceLock::new();
    TEXTS.get_or_init(|| serde_json::from_str(ENGLISH).unwrap_or(Value::Null))
}

/// The English text of an i18n key such as `files.groups.temp.title`, or the key itself.
fn text(key: &str) -> String {
    english()
        .pointer(&format!("/{}", key.replace('.', "/")))
        .and_then(Value::as_str)
        .map_or_else(|| key.to_string(), str::to_string)
}

/// Icons (pictures for the window) go; every `somethingKey` becomes `something` in
/// words, and a `{ "key": ... }` name becomes `{ "text": ... }`.
fn readable(mut value: Value) -> Value {
    translate(&mut value);
    value
}

fn translate(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(translate),
        Value::Object(map) => {
            map.remove("icon");
            map.remove("iconColor");
            if map.len() == 1 {
                if let Some(Value::String(key)) = map.get("key") {
                    let words = text(key);
                    map.clear();
                    map.insert("text".into(), Value::String(words));
                    return;
                }
            }
            let keyed: Vec<String> = map
                .keys()
                .filter(|k| k.len() > 3 && k.ends_with("Key"))
                .cloned()
                .collect();
            for k in keyed {
                let Some(v) = map.remove(&k) else { continue };
                let plain = k.trim_end_matches("Key").to_string();
                if let Value::String(key) = v {
                    if map.get(&plain).is_none_or(Value::is_null) {
                        map.insert(plain, Value::String(text(&key)));
                    }
                }
            }
            map.values_mut().for_each(translate);
        }
        _ => {}
    }
}

/// Turns a short key such as a leftover's `"programFolder"` into words, in every item of a list.
fn prefix_keys(value: &mut Value, field: &str, prefix: &str) {
    for item in value.as_array_mut().into_iter().flatten() {
        if let Some(Value::String(key)) = item.get(field) {
            let words = text(&format!("{prefix}{key}"));
            item[field] = Value::String(words);
        }
    }
}

/// Keeps the first `max` items of `value[field]`, noting how many were left out.
fn truncate(value: &mut Value, field: &str, max: usize) {
    let Some(Value::Array(list)) = value.get_mut(field) else {
        return;
    };
    if list.len() > max {
        let left_out = list.len() - max;
        list.truncate(max);
        let note = format!("{field}LeftOut");
        value[note] = json!(left_out);
    }
}

// ---------------------------------------------------------------------------
// Starting the server.

/// When started with `--mcp`, serves on stdin and stdout until the client closes them,
/// and returns the exit code; `None` means start the app normally. `build` makes the
/// app without opening a window: only its state and data folder are used.
pub fn run_from_args(build: impl FnOnce() -> tauri::Result<tauri::App>) -> Option<i32> {
    if std::env::args_os().nth(1)? != FLAG {
        return None;
    }
    let app = match build() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("Easy Uninstaller: {e}");
            return Some(1);
        }
    };
    let handle = app.handle().clone();
    let dir = match init(&handle) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("Easy Uninstaller: {e}");
            return Some(1);
        }
    };
    let tools = AppTools { app: handle, dir };
    serve(&tools, std::io::stdin().lock(), std::io::stdout());
    drop(app);
    Some(0)
}

/// The state the tools read, without what the window's start does on top (deleting
/// expired Quarantine entries, reminders, the local index).
fn init(app: &AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = app.path().app_local_data_dir()?;
    app.manage(crate::quarantine::Quarantine::new(dir.join("Quarantine")));
    crate::history::init(app)?;
    crate::review::init(app)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    struct Fake {
        on: AtomicBool,
        seen: Mutex<Vec<(String, u64)>>,
    }

    impl Fake {
        fn new(on: bool) -> Self {
            Fake {
                on: AtomicBool::new(on),
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    impl Tools for Fake {
        fn enabled(&self) -> bool {
            self.on.load(Ordering::Relaxed)
        }
        fn list(&self) -> Value {
            definitions()
        }
        fn call(
            &self,
            name: &str,
            args: &Value,
            progress: &dyn Fn(&str),
        ) -> Result<Value, CallError> {
            match name {
                "get_disk_space" => {
                    progress("half way");
                    Ok(json!({ "free": 5 }))
                }
                "explain_folder" if args.get("path").is_none() => {
                    Err(CallError::Failed("path is required.".into()))
                }
                "explain_folder" => Ok(json!({ "known": false })),
                _ => Err(CallError::Unknown),
            }
        }
        fn seen(&self, client: &str, calls: u64) {
            self.seen.lock().unwrap().push((client.to_string(), calls));
        }
    }

    /// Sends the lines and returns every message the server wrote, by id (notifications under "note").
    fn run(tools: &Fake, lines: &[Value]) -> Vec<Value> {
        let input: String = lines.iter().map(|l| format!("{l}\n")).collect();
        let mut out = Vec::new();
        serve(tools, input.as_bytes(), &mut out);
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn by_id(out: &[Value], id: u64) -> &Value {
        out.iter()
            .find(|m| m["id"] == json!(id))
            .expect("an answer")
    }

    fn init_msg(version: &str) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": version, "capabilities": {}, "clientInfo": { "name": "Claude Desktop", "version": "1" } } })
    }

    #[test]
    fn initializes_with_the_clients_version_or_the_newest() {
        let fake = Fake::new(true);
        let out = run(&fake, &[init_msg("2025-06-18")]);
        let r = &by_id(&out, 1)["result"];
        assert_eq!(r["protocolVersion"], "2025-06-18");
        assert_eq!(r["serverInfo"]["name"], "easy-uninstaller");
        assert!(r["capabilities"]["tools"].is_object());
        assert_eq!(fake.seen.lock().unwrap()[0], ("Claude Desktop".into(), 0));

        let out = run(&fake, &[init_msg("1999-01-01")]);
        assert_eq!(
            by_id(&out, 1)["result"]["protocolVersion"],
            PROTOCOL_VERSIONS[0]
        );
    }

    #[test]
    fn refuses_while_off() {
        let fake = Fake::new(false);
        let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "get_disk_space" } });
        let out = run(&fake, &[init_msg("2025-06-18"), call]);
        assert_eq!(by_id(&out, 1)["error"]["message"], OFF);
        assert_eq!(by_id(&out, 2)["result"]["isError"], true);
        assert!(fake.seen.lock().unwrap().is_empty());
    }

    #[test]
    fn lists_read_only_tools() {
        let fake = Fake::new(true);
        let out = run(
            &fake,
            &[json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" })],
        );
        let tools = by_id(&out, 3)["result"]["tools"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(tools.len(), 15);
        let mut names = std::collections::HashSet::new();
        for t in &tools {
            assert!(
                names.insert(t["name"].as_str().unwrap().to_string()),
                "unique names"
            );
            assert_eq!(t["inputSchema"]["type"], "object");
            assert_eq!(t["annotations"]["readOnlyHint"], true);
            assert_eq!(t["annotations"]["destructiveHint"], false);
        }
    }

    #[test]
    fn calls_tools_and_reports_progress() {
        let fake = Fake::new(true);
        let out = run(
            &fake,
            &[
                init_msg("2025-06-18"),
                json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {
                    "name": "get_disk_space", "arguments": {}, "_meta": { "progressToken": "p1" } } }),
                json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "explain_folder" } }),
                json!({ "jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": { "name": "delete_everything" } }),
                json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" }),
                json!({ "jsonrpc": "2.0", "id": 8, "method": "resources/list" }),
            ],
        );
        let text = by_id(&out, 4)["result"]["content"][0]["text"]
            .as_str()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(text).unwrap(),
            json!({ "free": 5 })
        );
        let note = out
            .iter()
            .find(|m| m["method"] == "notifications/progress")
            .unwrap();
        assert_eq!(note["params"]["progressToken"], "p1");
        assert_eq!(note["params"]["progress"], 1);
        assert_eq!(note["params"]["message"], "half way");

        assert_eq!(by_id(&out, 5)["result"]["isError"], true);
        assert_eq!(by_id(&out, 6)["error"]["code"], -32602);
        assert_eq!(by_id(&out, 7)["result"], json!({}));
        assert_eq!(by_id(&out, 8)["error"]["code"], -32601);
        // Only the initialized notification went unanswered.
        assert_eq!(out.len(), 7);
        let seen = fake.seen.lock().unwrap();
        assert_eq!(seen.iter().map(|s| s.1).max(), Some(3));
    }

    #[test]
    fn answers_bad_lines() {
        let input = "not json\n[1,2]\n\n";
        let mut out = Vec::new();
        serve(&Fake::new(true), input.as_bytes(), &mut out);
        let out: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(out[0]["error"]["code"], -32700);
        assert_eq!(out[1]["error"]["code"], -32600);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn makes_answers_readable() {
        let v = readable(json!([{
            "titleKey": "files.groups.temp.title",
            "icon": "data:image/png;base64,AAAA",
            "iconColor": "#fff",
            "labelKey": null,
            "label": "Kept",
            "names": [{ "key": "files.groups.temp.title" }, { "text": "Spotify" }],
            "nested": { "descriptionKey": "no.such.key" },
        }]));
        assert_eq!(
            v,
            json!([{
                "title": "Temporary files and caches",
                "label": "Kept",
                "names": [{ "text": "Temporary files and caches" }, { "text": "Spotify" }],
                "nested": { "description": "no.such.key" },
            }])
        );
    }

    #[test]
    fn short_keys_and_long_lists() {
        let mut v = json!([{ "reason": "programFolder", "items": [1, 2, 3] }, { "reason": 3 }]);
        prefix_keys(&mut v, "reason", "leftovers.reasons.");
        assert_ne!(v[0]["reason"], "programFolder");
        assert!(!v[0]["reason"].as_str().unwrap().starts_with("leftovers."));
        assert_eq!(v[1]["reason"], 3);
        truncate(&mut v[0], "items", 2);
        assert_eq!(v[0]["items"], json!([1, 2]));
        assert_eq!(v[0]["itemsLeftOut"], 1);
        truncate(&mut v[0], "items", 5);
        assert_eq!(v[0]["itemsLeftOut"], 1);
    }

    #[test]
    fn every_short_key_has_words() {
        // The texts these tools look up exist in en.json.
        for key in [
            "files.groups.temp.title",
            "leftovers.reasons.registry",
            "startup.notes.background",
            "map.blocks.caches",
        ] {
            assert_ne!(text(key), key, "{key}");
        }
        let known = crate::explain::KnownFolders::new(|_| {
            Some(PathBuf::from(r"C:\Users\Test\AppData\Local"))
        });
        let any = known.lookup(Path::new(r"C:\Users\Test\AppData\Local\Temp\x"));
        if let Some(folder) = any {
            let v = explain(Some(folder));
            assert_eq!(v["known"], true);
            assert!(v["what"].is_string());
        }
        assert_eq!(explain(None), json!({ "known": false }));
    }

    #[test]
    fn settings_are_off_until_turned_on() {
        let dir = std::env::temp_dir().join(format!("eu-mcp-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert!(!is_enabled(&dir));
        write_json(&dir.join(SETTINGS_FILE), &Settings { enabled: true }).unwrap();
        assert!(is_enabled(&dir));
        let client = McpClient {
            name: "x".into(),
            last_seen: 1,
            calls: 2,
        };
        write_json(&dir.join(CLIENT_FILE), &client).unwrap();
        let s = status(&dir);
        assert!(s.enabled);
        assert_eq!(s.last_client, Some(client));
        fs::write(dir.join(SETTINGS_FILE), "garbage").unwrap();
        assert!(!is_enabled(&dir));
        let _ = fs::remove_dir_all(&dir);
    }
}
