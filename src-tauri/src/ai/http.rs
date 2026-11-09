//! The few HTTP calls the AI help makes: one question to an online service or a
//! model server on this PC, and its list of models.
//!
//! On Windows they go through WinHTTP, which ships with the system: it brings
//! TLS, the system proxy and certificates without adding a single crate.
//! Elsewhere (where the app is only built and tested) every call fails.

use std::time::Duration;

/// Answers bigger than this are cut off: an answer is a few kilobytes, a model
/// list a few hundred.
#[cfg_attr(not(windows), allow(dead_code))]
pub const MAX_BODY: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub https: bool,
    pub host: String,
    pub port: u16,
    /// Path and query, starting with `/`.
    pub path: String,
}

impl Url {
    /// `http(s)://host[:port][/path]`. Anything else (user info, spaces,
    /// another scheme) is refused.
    pub fn parse(text: &str) -> Option<Url> {
        let text = text.trim();
        let (https, rest) = match text.strip_prefix("https://") {
            Some(r) => (true, r),
            None => (false, text.strip_prefix("http://")?),
        };
        if rest.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return None;
        }
        let (authority, path) = match rest.find(['/', '?']) {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        if authority.is_empty() || authority.contains('@') {
            return None;
        }
        let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
            let end = v6.find(']')?;
            let host = &v6[..end];
            let after = &v6[end + 1..];
            let port = match after.strip_prefix(':') {
                Some(p) => Some(p),
                None if after.is_empty() => None,
                None => return None,
            };
            (format!("[{host}]"), port)
        } else {
            match authority.rsplit_once(':') {
                Some((h, p)) => (h.to_string(), Some(p)),
                None => (authority.to_string(), None),
            }
        };
        let host_ok = !host.is_empty()
            && host.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '[' | ']' | ':')
            });
        if !host_ok {
            return None;
        }
        let port = match port {
            Some(p) => p.parse::<u16>().ok().filter(|p| *p != 0)?,
            None if https => 443,
            None => 80,
        };
        let path = if path.starts_with('?') {
            format!("/{path}")
        } else if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        };
        Some(Url {
            https,
            host: host.to_ascii_lowercase(),
            port,
            path,
        })
    }

    /// A server on this PC: `localhost`, `127.x.x.x` or `::1`.
    pub fn is_loopback(&self) -> bool {
        let h = self.host.as_str();
        h == "localhost"
            || h.ends_with(".localhost")
            || h == "[::1]"
            || h.strip_prefix("127.").is_some_and(|r| {
                r.split('.').count() == 3 && r.split('.').all(|p| p.parse::<u8>().is_ok())
            })
    }

    #[cfg(test)]
    pub fn to_text(&self) -> String {
        let scheme = if self.https { "https" } else { "http" };
        let default = if self.https { 443 } else { 80 };
        if self.port == default {
            format!("{scheme}://{}{}", self.host, self.path)
        } else {
            format!("{scheme}://{}:{}{}", self.host, self.port, self.path)
        }
    }

    /// `base` with `/more` added to its path: `…/v1` + `models` = `…/v1/models`.
    pub fn join(&self, more: &str) -> Url {
        let mut out = self.clone();
        out.path = format!(
            "{}/{}",
            self.path.trim_end_matches('/'),
            more.trim_start_matches('/')
        );
        out
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum HttpError {
    /// Nobody answered at that address (the server is off, or no network).
    Offline,
    #[cfg_attr(not(windows), allow(dead_code))]
    Timeout,
    Other(String),
}

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

/// One request, blocking. `headers` must hold no line breaks (callers check
/// what the user typed). `body` is sent as JSON.
pub fn request(
    method: &str,
    url: &Url,
    headers: &[(&str, String)],
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<Response, HttpError> {
    if headers
        .iter()
        .any(|(k, v)| k.contains(['\r', '\n', ':']) || v.contains(['\r', '\n']))
    {
        return Err(HttpError::Other("bad header".into()));
    }
    platform::request(method, url, headers, body, timeout)
}

#[cfg(windows)]
mod platform {
    use super::{HttpError, Response, Url, MAX_BODY};
    use std::ffi::c_void;
    use std::time::Duration;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Networking::WinHttp::*;

    /// Closes a WinHTTP handle when dropped.
    struct Handle(*mut c_void);

    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    fn fail(e: windows::core::Error) -> HttpError {
        // WinHTTP errors come back as HRESULT_FROM_WIN32(code).
        let code = (e.code().0 as u32) & 0xFFFF;
        match code {
            ERROR_WINHTTP_TIMEOUT => HttpError::Timeout,
            ERROR_WINHTTP_CANNOT_CONNECT | ERROR_WINHTTP_NAME_NOT_RESOLVED => HttpError::Offline,
            // ERROR_WINHTTP_CONNECTION_ERROR: the server closed the connection.
            12030 => HttpError::Offline,
            _ => HttpError::Other(e.message()),
        }
    }

    fn last_error() -> HttpError {
        fail(windows::core::Error::from_thread())
    }

    pub fn request(
        method: &str,
        url: &Url,
        headers: &[(&str, String)],
        body: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<Response, HttpError> {
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        unsafe {
            let session = Handle(WinHttpOpen(
                &HSTRING::from("EasyUninstaller/1"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ));
            if session.0.is_null() {
                return Err(last_error());
            }
            WinHttpSetTimeouts(session.0, ms, ms.min(15_000), ms, ms).map_err(fail)?;
            let host = url.host.trim_start_matches('[').trim_end_matches(']');
            let connect = Handle(WinHttpConnect(session.0, &HSTRING::from(host), url.port, 0));
            if connect.0.is_null() {
                return Err(last_error());
            }
            let flags = if url.https {
                WINHTTP_FLAG_SECURE
            } else {
                WINHTTP_OPEN_REQUEST_FLAGS(0)
            };
            let req = Handle(WinHttpOpenRequest(
                connect.0,
                &HSTRING::from(method),
                &HSTRING::from(url.path.as_str()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                flags,
            ));
            if req.0.is_null() {
                return Err(last_error());
            }
            let mut head = String::new();
            for (k, v) in headers {
                head.push_str(&format!("{k}: {v}\r\n"));
            }
            if body.is_some() {
                head.push_str("Content-Type: application/json\r\n");
            }
            let head: Vec<u16> = head.encode_utf16().collect();
            let (ptr, len) = match body {
                Some(b) => (Some(b.as_ptr() as *const c_void), b.len() as u32),
                None => (None, 0),
            };
            WinHttpSendRequest(
                req.0,
                (!head.is_empty()).then_some(head.as_slice()),
                ptr,
                len,
                len,
                0,
            )
            .map_err(fail)?;
            WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(fail)?;

            let mut status = 0u32;
            let mut size = std::mem::size_of::<u32>() as u32;
            WinHttpQueryHeaders(
                req.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some(&mut status as *mut u32 as *mut c_void),
                &mut size,
                std::ptr::null_mut(),
            )
            .map_err(fail)?;

            let mut out = Vec::new();
            let mut chunk = vec![0u8; 64 * 1024];
            loop {
                let mut read = 0u32;
                WinHttpReadData(
                    req.0,
                    chunk.as_mut_ptr() as *mut c_void,
                    chunk.len() as u32,
                    &mut read,
                )
                .map_err(fail)?;
                if read == 0 {
                    break;
                }
                out.extend_from_slice(&chunk[..read as usize]);
                if out.len() > MAX_BODY {
                    return Err(HttpError::Other("answer too big".into()));
                }
            }
            Ok(Response {
                status: status as u16,
                body: out,
            })
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{HttpError, Response, Url};
    use std::time::Duration;

    pub fn request(
        _method: &str,
        _url: &Url,
        _headers: &[(&str, String)],
        _body: Option<&[u8]>,
        _timeout: Duration,
    ) -> Result<Response, HttpError> {
        Err(HttpError::Offline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_read_strictly() {
        let u = Url::parse("https://api.openai.com/v1").unwrap();
        assert_eq!(
            (u.https, u.host.as_str(), u.port, u.path.as_str()),
            (true, "api.openai.com", 443, "/v1")
        );
        assert_eq!(
            u.join("models").to_text(),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(u.join("/models").path, "/v1/models");

        let l = Url::parse("http://LOCALHOST:11434").unwrap();
        assert_eq!(
            (l.host.as_str(), l.port, l.path.as_str()),
            ("localhost", 11434, "/")
        );
        assert!(l.is_loopback());
        assert_eq!(l.to_text(), "http://localhost:11434/");
        assert!(Url::parse("http://127.0.0.1:1234/v1")
            .unwrap()
            .is_loopback());
        assert!(Url::parse("http://[::1]:8000/v1").unwrap().is_loopback());
        assert!(!Url::parse("http://127.0.0.1.evil.com/")
            .unwrap()
            .is_loopback());
        assert!(!Url::parse("http://192.168.1.4:8000").unwrap().is_loopback());

        for bad in [
            "ftp://x",
            "localhost:11434",
            "http://user:pw@host/",
            "http://host/a b",
            "http://:80/",
            "http://host:0/",
            "http://host:99999/",
            "http://ho st/",
            "http://host\r\nX: y/",
        ] {
            assert_eq!(Url::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn line_breaks_never_reach_a_header() {
        let url = Url::parse("http://127.0.0.1:9/").unwrap();
        let r = request(
            "GET",
            &url,
            &[("Authorization", "Bearer x\r\nEvil: 1".into())],
            None,
            Duration::from_secs(1),
        );
        assert!(matches!(r, Err(HttpError::Other(_))));
    }
}
