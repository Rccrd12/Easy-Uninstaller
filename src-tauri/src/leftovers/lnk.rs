//! Reads where a Windows shortcut (`.lnk`) points, following the published file
//! format ([MS-SHLLINK]). Only the plain local path is read: shortcuts to network
//! places or special folders give `None`, and are then never treated as broken.

use std::path::PathBuf;

const HEADER_SIZE: usize = 0x4C;
const HAS_ID_LIST: u32 = 0x1;
const HAS_LINK_INFO: u32 = 0x2;
const VOLUME_ID_AND_LOCAL_BASE_PATH: u32 = 0x1;

fn u16_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// A zero-terminated ANSI string. Non-ASCII text depends on the PC's code page,
/// so it is not guessed: `None`.
fn ansi_at(b: &[u8], at: usize) -> Option<String> {
    let bytes = b.get(at..)?;
    let end = bytes.iter().position(|&c| c == 0)?;
    let s = &bytes[..end];
    s.is_ascii()
        .then(|| String::from_utf8_lossy(s).into_owned())
}

/// A zero-terminated UTF-16 string.
fn unicode_at(b: &[u8], at: usize) -> Option<String> {
    let mut units = Vec::new();
    let mut i = at;
    loop {
        let u = u16_at(b, i)? as u16;
        if u == 0 {
            break;
        }
        units.push(u);
        i += 2;
    }
    String::from_utf16(&units).ok()
}

/// The local file or folder the shortcut opens.
pub fn target(b: &[u8]) -> Option<PathBuf> {
    if u32_at(b, 0)? as usize != HEADER_SIZE {
        return None;
    }
    let flags = u32_at(b, 0x14)?;
    let mut at = HEADER_SIZE;
    if flags & HAS_ID_LIST != 0 {
        at += 2 + u16_at(b, at)?;
    }
    if flags & HAS_LINK_INFO == 0 {
        return None;
    }
    let info = at;
    let header_size = u32_at(b, info + 4)? as usize;
    let info_flags = u32_at(b, info + 8)?;
    if info_flags & VOLUME_ID_AND_LOCAL_BASE_PATH == 0 {
        return None;
    }
    let suffix_offset = u32_at(b, info + 0x18)? as usize;
    let (base, suffix) = if header_size >= 0x24 {
        let base = u32_at(b, info + 0x1C)? as usize;
        let suffix = u32_at(b, info + 0x20)? as usize;
        (unicode_at(b, info + base)?, unicode_at(b, info + suffix)?)
    } else {
        let base = u32_at(b, info + 0x10)? as usize;
        (ansi_at(b, info + base)?, ansi_at(b, info + suffix_offset)?)
    };
    if base.is_empty() {
        return None;
    }
    let mut path = base;
    if !suffix.is_empty() {
        if !path.ends_with('\\') {
            path.push('\\');
        }
        path.push_str(&suffix);
    }
    Some(PathBuf::from(path))
}

/// Builds a minimal shortcut to `path` (ANSI local path), as Windows writes them.
#[cfg(test)]
pub fn sample(path: &str, with_id_list: bool) -> Vec<u8> {
    sample_bytes(path.as_bytes(), with_id_list)
}

#[cfg(test)]
fn sample_bytes(path: &[u8], with_id_list: bool) -> Vec<u8> {
    let mut b = vec![0u8; HEADER_SIZE];
    b[0..4].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
    let flags = HAS_LINK_INFO | if with_id_list { HAS_ID_LIST } else { 0 };
    b[0x14..0x18].copy_from_slice(&flags.to_le_bytes());
    if with_id_list {
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&[1, 2, 3, 4]);
    }
    let volume = [16u32, 3, 0, 16]; // size, type, serial, label offset
    let volume: Vec<u8> = volume
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .chain([0u8])
        .collect();
    let header = 0x1Cu32;
    let base_offset = header + volume.len() as u32;
    let suffix_offset = base_offset + path.len() as u32 + 1;
    let size = suffix_offset + 1;
    for v in [size, header, 1, header, base_offset, 0, suffix_offset] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&volume);
    b.extend_from_slice(path);
    b.extend_from_slice(&[0, 0]);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_local_target() {
        for id_list in [false, true] {
            let b = sample(r"C:\Program Files\Zoom\bin\Zoom.exe", id_list);
            assert_eq!(
                target(&b),
                Some(PathBuf::from(r"C:\Program Files\Zoom\bin\Zoom.exe"))
            );
        }
    }

    #[test]
    fn unknown_or_damaged_shortcuts_give_nothing() {
        assert_eq!(target(b""), None);
        assert_eq!(target(&[0u8; 200]), None);
        let mut b = sample(r"C:\x.exe", false);
        b.truncate(b.len() - 6);
        assert_eq!(target(&b), None);
        let mut no_info = sample(r"C:\x.exe", false);
        no_info[0x14] = 0;
        assert_eq!(target(&no_info), None);
        // Non-ASCII ANSI text depends on the code page: not guessed.
        assert_eq!(target(&sample_bytes(b"C:\\Caf\xe9\\x.exe", false)), None);
    }
}
