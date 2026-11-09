//! API keys, kept in Windows' Credential Manager (one generic credential per
//! service, readable only by this Windows account), never in the app's files.
//! Elsewhere they are kept in memory, for tests.

/// What a key may look like: printable ASCII without spaces, so it can never
/// break out of an HTTP header.
pub fn valid(key: &str) -> bool {
    (8..=512).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_graphic())
}

fn target(service: &str) -> String {
    format!("EasyUninstaller/ai/{service}")
}

pub fn set(service: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if !valid(key) {
        return Err("ai.error.keyFormat".into());
    }
    platform::write(&target(service), key)
}

pub fn get(service: &str) -> Option<String> {
    platform::read(&target(service)).filter(|k| valid(k))
}

pub fn remove(service: &str) {
    platform::delete(&target(service));
}

#[cfg(windows)]
mod platform {
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Security::Credentials::*;

    pub fn write(target: &str, key: &str) -> Result<(), String> {
        let mut name: Vec<u16> = target.encode_utf16().chain([0]).collect();
        let mut user: Vec<u16> = "EasyUninstaller".encode_utf16().chain([0]).collect();
        let mut blob = key.as_bytes().to_vec();
        let cred = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(name.as_mut_ptr()),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: PWSTR(user.as_mut_ptr()),
            ..Default::default()
        };
        unsafe { CredWriteW(&cred, 0) }.map_err(|e| e.message())
    }

    pub fn read(target: &str) -> Option<String> {
        let mut found: *mut CREDENTIALW = std::ptr::null_mut();
        unsafe {
            CredReadW(&HSTRING::from(target), CRED_TYPE_GENERIC, None, &mut found).ok()?;
            if found.is_null() {
                return None;
            }
            let c = &*found;
            let bytes = if c.CredentialBlob.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
            };
            CredFree(found as *const _);
            String::from_utf8(bytes).ok()
        }
    }

    pub fn delete(target: &str) {
        unsafe {
            let _ = CredDeleteW(&HSTRING::from(target), CRED_TYPE_GENERIC, None);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::collections::HashMap;
    use std::sync::Mutex;

    static KEYS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

    pub fn write(target: &str, key: &str) -> Result<(), String> {
        let mut keys = KEYS.lock().map_err(|e| e.to_string())?;
        keys.get_or_insert_with(HashMap::new)
            .insert(target.into(), key.into());
        Ok(())
    }

    pub fn read(target: &str) -> Option<String> {
        KEYS.lock().ok()?.as_ref()?.get(target).cloned()
    }

    pub fn delete(target: &str) {
        if let Ok(mut keys) = KEYS.lock() {
            if let Some(k) = keys.as_mut() {
                k.remove(target);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_stored_checked_and_removed() {
        assert!(valid("sk-ant-api03-abcdef"));
        assert!(!valid("short"));
        assert!(!valid("sk-has space-123"));
        assert!(!valid("sk-line\r\nbreak-123"));
        assert!(!valid("sk-àccentato-1234"));
        assert_eq!(
            set("test-svc", "bad key here"),
            Err("ai.error.keyFormat".into())
        );
        set("test-svc", "  sk-test-12345678  ").unwrap();
        assert_eq!(get("test-svc").as_deref(), Some("sk-test-12345678"));
        remove("test-svc");
        assert_eq!(get("test-svc"), None);
    }
}
