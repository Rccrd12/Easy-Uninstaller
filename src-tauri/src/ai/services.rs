//! Online services reached with an API key, and model servers on this PC
//! (Ollama, LM Studio, Unsloth Studio, any OpenAI-compatible one).
//!
//! Each question is one request with no tools: the text goes out, the answer
//! comes back. Online services have fixed addresses; the "other service" one
//! must use HTTPS, and a server "on this PC" must really be on this PC.

use super::http::{self, HttpError, Url};
use super::{keys, AiModel};
use serde_json::{json, Value};
use std::time::Duration;

/// How a service is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A program the user signed in to with their subscription.
    Cli,
    /// An online service, with an API key.
    Api,
    /// A model server on this PC.
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    Anthropic,
    OpenAi,
    None,
}

pub struct Service {
    pub id: &'static str,
    pub kind: Kind,
    pub wire: Wire,
    /// Where it answers, unless the user gives an address.
    pub url: &'static str,
    /// A second address tried when the first does not answer.
    pub other_url: &'static str,
    /// It cannot answer without a key.
    pub needs_key: bool,
}

pub const SERVICES: &[Service] = &[
    Service {
        id: "claude-code",
        kind: Kind::Cli,
        wire: Wire::None,
        url: "",
        other_url: "",
        needs_key: false,
    },
    Service {
        id: "codex",
        kind: Kind::Cli,
        wire: Wire::None,
        url: "",
        other_url: "",
        needs_key: false,
    },
    Service {
        id: "antigravity-cli",
        kind: Kind::Cli,
        wire: Wire::None,
        url: "",
        other_url: "",
        needs_key: false,
    },
    Service {
        id: "anthropic",
        kind: Kind::Api,
        wire: Wire::Anthropic,
        url: "https://api.anthropic.com/v1",
        other_url: "",
        needs_key: true,
    },
    Service {
        id: "openai",
        kind: Kind::Api,
        wire: Wire::OpenAi,
        url: "https://api.openai.com/v1",
        other_url: "",
        needs_key: true,
    },
    Service {
        id: "google",
        kind: Kind::Api,
        wire: Wire::OpenAi,
        url: "https://generativelanguage.googleapis.com/v1beta/openai",
        other_url: "",
        needs_key: true,
    },
    Service {
        id: "openrouter",
        kind: Kind::Api,
        wire: Wire::OpenAi,
        url: "https://openrouter.ai/api/v1",
        other_url: "",
        needs_key: true,
    },
    Service {
        id: "custom",
        kind: Kind::Api,
        wire: Wire::OpenAi,
        url: "",
        other_url: "",
        needs_key: false,
    },
    Service {
        id: "ollama",
        kind: Kind::Local,
        wire: Wire::OpenAi,
        url: "http://127.0.0.1:11434/v1",
        other_url: "",
        needs_key: false,
    },
    Service {
        id: "lmstudio",
        kind: Kind::Local,
        wire: Wire::OpenAi,
        url: "http://127.0.0.1:1234/v1",
        other_url: "",
        needs_key: false,
    },
    // Unsloth Studio takes 8888 most often, else 8000; it wants its sk-unsloth key.
    Service {
        id: "unsloth",
        kind: Kind::Local,
        wire: Wire::OpenAi,
        url: "http://127.0.0.1:8888/v1",
        other_url: "http://127.0.0.1:8000/v1",
        needs_key: true,
    },
    Service {
        id: "local-custom",
        kind: Kind::Local,
        wire: Wire::OpenAi,
        url: "",
        other_url: "",
        needs_key: false,
    },
];

pub fn find(id: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|s| s.id == id)
}

/// A model name safe to send and to put on a command line.
pub fn safe_model(model: &str) -> Option<&str> {
    let m = model.trim();
    (!m.is_empty()
        && m.len() <= 128
        && m.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | ':' | '/' | '[' | ']' | '@')
        }))
    .then_some(m)
}

/// The address to use for `service`: its own for online services, the user's
/// (or the usual one) for the others. `localhost:11434` becomes
/// `http://127.0.0.1:11434/v1`.
pub fn address(service: &Service, typed: &str) -> Result<Vec<Url>, String> {
    let bad = || "ai.error.badAddress".to_string();
    if service.kind == Kind::Api && !service.url.is_empty() {
        return Ok(vec![Url::parse(service.url).ok_or_else(bad)?]);
    }
    let typed = typed.trim();
    if typed.is_empty() {
        if service.url.is_empty() {
            return Err(bad());
        }
        let mut out = vec![Url::parse(service.url).ok_or_else(bad)?];
        if let Some(u) = Url::parse(service.other_url) {
            out.push(u);
        }
        return Ok(out);
    }
    let with_scheme = if typed.contains("://") {
        typed.to_string()
    } else if service.kind == Kind::Local {
        format!("http://{typed}")
    } else {
        format!("https://{typed}")
    };
    let mut url = Url::parse(&with_scheme).ok_or_else(bad)?;
    if url.host == "localhost" {
        url.host = "127.0.0.1".into();
    }
    match service.kind {
        Kind::Local if !url.is_loopback() => return Err("ai.error.notLocal".into()),
        Kind::Api if !url.https && !url.is_loopback() => return Err("ai.error.needsHttps".into()),
        _ => {}
    }
    let path = url.path.trim_end_matches('/').to_string();
    url.path = if path.is_empty() { "/v1".into() } else { path };
    Ok(vec![url])
}

fn headers(service: &Service, key: Option<&str>) -> Vec<(&'static str, String)> {
    let mut h = vec![("Accept", "application/json".to_string())];
    match (service.wire, key) {
        (Wire::Anthropic, Some(k)) => {
            h.push(("x-api-key", k.to_string()));
            h.push(("anthropic-version", "2023-06-01".to_string()));
        }
        (_, Some(k)) => h.push(("Authorization", format!("Bearer {k}"))),
        _ => {}
    }
    if service.id == "openrouter" {
        h.push(("X-Title", "Easy Uninstaller".to_string()));
    }
    h
}

fn key_for(service: &Service) -> Result<Option<String>, String> {
    match keys::get(service.id) {
        Some(k) => Ok(Some(k)),
        None if service.needs_key => Err("ai.error.noKey".into()),
        None => Ok(None),
    }
}

/// Why a request failed, as an i18n key (and the server's words, after `|`).
fn http_error(e: HttpError) -> String {
    match e {
        HttpError::Offline => "ai.error.offline".into(),
        HttpError::Timeout => "ai.error.timeout".into(),
        HttpError::Other(m) => format!("ai.error.server|{m}"),
    }
}

/// The message in an error answer, short.
fn server_message(body: &[u8]) -> String {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let msg = v["error"]["message"]
        .as_str()
        .or_else(|| v["error"].as_str())
        .or_else(|| v["message"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(&body[..body.len().min(200)]).into_owned());
    msg.chars().filter(|c| !c.is_control()).take(200).collect()
}

fn check_status(r: &http::Response) -> Result<(), String> {
    match r.status {
        200..=299 => Ok(()),
        401 | 403 => Err("ai.error.badKey".into()),
        404 => Err(format!("ai.error.notFound|{}", server_message(&r.body))),
        429 => Err("ai.error.tooMany".into()),
        _ => Err(format!(
            "ai.error.server|{} {}",
            r.status,
            server_message(&r.body)
        )),
    }
}

/// The request body for one question.
pub fn ask_body(service: &Service, model: &str, question: &str) -> Value {
    match service.wire {
        Wire::Anthropic => json!({
            "model": model,
            "max_tokens": 2048,
            "messages": [{ "role": "user", "content": question }],
        }),
        _ => json!({
            "model": model,
            "messages": [{ "role": "user", "content": question }],
            "stream": false,
        }),
    }
}

/// The text of an answer.
pub fn answer_text(service: &Service, body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let text = match service.wire {
        Wire::Anthropic => v["content"]
            .as_array()?
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => {
            let content = &v["choices"][0]["message"]["content"];
            match content {
                Value::String(s) => s.clone(),
                Value::Array(parts) => parts
                    .iter()
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join(""),
                _ => return None,
            }
        }
    };
    (!text.trim().is_empty()).then_some(text)
}

/// Models this service offers for a short question, from its `/models` list.
pub fn parse_models(service: &Service, body: &[u8]) -> Vec<AiModel> {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let list = v["data"]
        .as_array()
        .or_else(|| v["models"].as_array())
        .cloned()
        .unwrap_or_default();
    // Models that do not chat: pictures, speech, embeddings…
    const NOT_CHAT: &[&str] = &[
        "embed",
        "tts",
        "whisper",
        "dall-e",
        "image",
        "audio",
        "realtime",
        "moderation",
        "transcribe",
        "davinci",
        "babbage",
        "search",
        "veo",
        "imagen",
        "aqa",
    ];
    let mut out: Vec<AiModel> = Vec::new();
    for m in list {
        let raw = m["id"]
            .as_str()
            .or_else(|| m["name"].as_str())
            .unwrap_or("");
        let id = raw.strip_prefix("models/").unwrap_or(raw);
        let Some(id) = safe_model(id) else { continue };
        let lower = id.to_ascii_lowercase();
        if service.kind == Kind::Api && NOT_CHAT.iter().any(|n| lower.contains(n)) {
            continue;
        }
        if service.id == "google" && !lower.starts_with("gemini") {
            continue;
        }
        let label = m["display_name"]
            .as_str()
            .or_else(|| m["displayName"].as_str())
            .or_else(|| m["name"].as_str().filter(|n| *n != raw))
            .map(|l| {
                l.chars()
                    .filter(|c| !c.is_control())
                    .take(80)
                    .collect::<String>()
            })
            .filter(|l| !l.trim().is_empty())
            .unwrap_or_else(|| id.to_string());
        if !out.iter().any(|o| o.id == id) {
            out.push(AiModel {
                id: id.to_string(),
                label,
            });
        }
        if out.len() >= 400 {
            break;
        }
    }
    out
}

const LIST_TIMEOUT: Duration = Duration::from_secs(15);
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

fn ask_timeout(service: &Service) -> Duration {
    // A model on this PC can be slow, above all the first time it loads.
    if service.kind == Kind::Local {
        Duration::from_secs(300)
    } else {
        Duration::from_secs(120)
    }
}

/// The models of `service` at `typed` (the user's address, if any).
pub fn models(service: &Service, typed: &str) -> Result<Vec<AiModel>, String> {
    let key = key_for(service)?;
    let mut last = "ai.error.offline".to_string();
    for base in address(service, typed)? {
        let url = if service.wire == Wire::Anthropic {
            base.join("models?limit=100")
        } else {
            base.join("models")
        };
        match http::request(
            "GET",
            &url,
            &headers(service, key.as_deref()),
            None,
            LIST_TIMEOUT,
        ) {
            Ok(r) => {
                check_status(&r)?;
                return Ok(parse_models(service, &r.body));
            }
            Err(e) => last = http_error(e),
        }
    }
    Err(last)
}

/// Whether the server on this PC answers, and how many models it has.
/// `None`: nobody answers. `Some(None)`: it answers but wants a key.
pub fn probe(service: &Service, typed: &str) -> Option<Option<usize>> {
    let key = keys::get(service.id);
    for base in address(service, typed).ok()? {
        let url = base.join("models");
        if let Ok(r) = http::request(
            "GET",
            &url,
            &headers(service, key.as_deref()),
            None,
            PROBE_TIMEOUT,
        ) {
            return Some(match r.status {
                200..=299 => Some(parse_models(service, &r.body).len()),
                _ => None,
            });
        }
    }
    None
}

/// One question, one answer.
pub fn ask(service: &Service, typed: &str, model: &str, question: &str) -> Result<String, String> {
    let model = safe_model(model).ok_or("ai.error.noModel")?;
    let key = key_for(service)?;
    let body = ask_body(service, model, question).to_string();
    let mut last = "ai.error.offline".to_string();
    for base in address(service, typed)? {
        let url = if service.wire == Wire::Anthropic {
            base.join("messages")
        } else {
            base.join("chat/completions")
        };
        match http::request(
            "POST",
            &url,
            &headers(service, key.as_deref()),
            Some(body.as_bytes()),
            ask_timeout(service),
        ) {
            Ok(r) => {
                check_status(&r)?;
                return answer_text(service, &r.body).ok_or_else(|| "ai.error.badAnswer".into());
            }
            Err(e) => last = http_error(e),
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str) -> &'static Service {
        find(id).unwrap()
    }

    #[test]
    fn addresses_are_fixed_online_and_local_on_this_pc() {
        let text = |id: &str, typed: &str| {
            address(s(id), typed).map(|v| v.iter().map(Url::to_text).collect::<Vec<_>>())
        };
        // The user's address is ignored for the known online services.
        assert_eq!(
            text("openai", "https://evil.example/v1").unwrap(),
            ["https://api.openai.com/v1"]
        );
        assert_eq!(text("ollama", "").unwrap(), ["http://127.0.0.1:11434/v1"]);
        assert_eq!(
            text("ollama", "localhost:11500").unwrap(),
            ["http://127.0.0.1:11500/v1"]
        );
        assert_eq!(
            text("unsloth", "").unwrap(),
            ["http://127.0.0.1:8888/v1", "http://127.0.0.1:8000/v1"]
        );
        assert_eq!(
            text("local-custom", "http://127.0.0.1:5000/api/v1/").unwrap(),
            ["http://127.0.0.1:5000/api/v1"]
        );
        assert_eq!(
            text("local-custom", "http://192.168.1.10:8000").unwrap_err(),
            "ai.error.notLocal"
        );
        assert_eq!(text("local-custom", "").unwrap_err(), "ai.error.badAddress");
        assert_eq!(
            text("custom", "api.groq.com/openai/v1").unwrap(),
            ["https://api.groq.com/openai/v1"]
        );
        assert_eq!(
            text("custom", "http://api.groq.com/v1").unwrap_err(),
            "ai.error.needsHttps"
        );
        assert_eq!(
            text("custom", "file:///etc/passwd").unwrap_err(),
            "ai.error.badAddress"
        );
    }

    #[test]
    fn model_names_are_checked() {
        assert_eq!(safe_model(" qwen3:8b "), Some("qwen3:8b"));
        assert_eq!(
            safe_model("anthropic/claude-sonnet-5.5"),
            Some("anthropic/claude-sonnet-5.5")
        );
        assert_eq!(
            safe_model("claude-opus-5-5[1m]"),
            Some("claude-opus-5-5[1m]")
        );
        assert_eq!(safe_model("x & calc"), None);
        assert_eq!(safe_model("a\"b"), None);
        assert_eq!(safe_model(""), None);
    }

    #[test]
    fn requests_and_answers_follow_each_wire() {
        let a = ask_body(s("anthropic"), "claude-haiku-5-5", "hi");
        assert_eq!(a["max_tokens"], 2048);
        assert_eq!(a["messages"][0]["content"], "hi");
        assert!(a.get("tools").is_none());
        let o = ask_body(s("ollama"), "qwen3:8b", "hi");
        assert_eq!(o["stream"], false);

        let h = headers(s("anthropic"), Some("sk-ant-1"));
        assert!(h.contains(&("x-api-key", "sk-ant-1".into())));
        assert!(h.iter().all(|(k, _)| *k != "Authorization"));
        let h = headers(s("openai"), Some("sk-1"));
        assert!(h.contains(&("Authorization", "Bearer sk-1".into())));

        let body = br#"{"content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"{\"verdict\":\"keep\"}"}]}"#;
        assert_eq!(
            answer_text(s("anthropic"), body).unwrap(),
            "{\"verdict\":\"keep\"}"
        );
        let body = br#"{"choices":[{"message":{"role":"assistant","content":"hello"}}]}"#;
        assert_eq!(answer_text(s("openrouter"), body).unwrap(), "hello");
        assert_eq!(answer_text(s("openai"), br#"{"choices":[]}"#), None);
    }

    #[test]
    fn model_lists_keep_chat_models() {
        let body = br#"{"data":[{"id":"gpt-5.5"},{"id":"text-embedding-3-small"},{"id":"gpt-image-2"},{"id":"bad name"}]}"#;
        let ids: Vec<_> = parse_models(s("openai"), body)
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, ["gpt-5.5"]);
        let body = br#"{"data":[{"id":"claude-sonnet-5-5","display_name":"Claude Sonnet 5.5"}]}"#;
        assert_eq!(
            parse_models(s("anthropic"), body)[0].label,
            "Claude Sonnet 5.5"
        );
        let body = br#"{"data":[{"id":"models/gemini-3-flash"},{"id":"models/embedding-001"},{"id":"models/imagen-4"}]}"#;
        let ids: Vec<_> = parse_models(s("google"), body)
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, ["gemini-3-flash"]);
        // On this PC every model is kept: the user chose what to download.
        let body = br#"{"object":"list","data":[{"id":"qwen3:8b"},{"id":"nomic-embed-text"}]}"#;
        assert_eq!(parse_models(s("ollama"), body).len(), 2);
    }

    #[test]
    fn statuses_become_messages() {
        let r = |status, body: &[u8]| http::Response {
            status,
            body: body.to_vec(),
        };
        assert_eq!(check_status(&r(401, b"")), Err("ai.error.badKey".into()));
        assert_eq!(check_status(&r(429, b"")), Err("ai.error.tooMany".into()));
        assert_eq!(
            check_status(&r(400, br#"{"error":{"message":"model not\nfound"}}"#)),
            Err("ai.error.server|400 model notfound".into())
        );
        assert!(check_status(&r(200, b"{}")).is_ok());
    }
}
