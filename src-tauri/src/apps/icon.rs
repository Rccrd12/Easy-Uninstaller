//! App icons as small `data:` URLs the UI can show directly.
//!
//! Classic programs: the icon in the file named by `DisplayIcon`, rendered at 32 px and
//! encoded as PNG by the tiny encoder below (no image library needed).
//! Store apps: their logo, which is already a PNG file.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::Path;

/// Logos bigger than this are not worth sending to the UI for a 32 px tile.
const MAX_LOGO_BYTES: u64 = 128 * 1024;

pub fn from_png_file(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_LOGO_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    bytes
        .starts_with(PNG_SIGNATURE)
        .then(|| png_data_url(&bytes))
}

#[cfg(windows)]
pub fn from_icon_location(location: &str) -> Option<String> {
    let (path, index) = parse_icon_location(location)?;
    let file = Path::new(&path);
    if !file.is_file() {
        return None;
    }
    if path.to_lowercase().ends_with(".png") {
        return from_png_file(file);
    }
    let (width, height, rgba) = windows_icon::extract(&path, index)?;
    Some(png_data_url(&encode_png(width, height, &rgba)))
}

#[cfg(not(windows))]
pub fn from_icon_location(_location: &str) -> Option<String> {
    None
}

/// A bitmap, such as a file thumbnail from Windows, as a PNG `data:` URL.
#[cfg(windows)]
pub fn from_bitmap(bitmap: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<String> {
    let (width, height, rgba) = windows_icon::bitmap_pixels(bitmap)?;
    Some(png_data_url(&encode_png(width, height, &rgba)))
}

/// `"C:\App\app.exe",0` / `C:\App\app.ico` / `%SystemRoot%\x.dll,-12` to (path, index).
pub fn parse_icon_location(raw: &str) -> Option<(String, i32)> {
    let raw = raw.trim();
    let (path, rest) = match raw.strip_prefix('"') {
        Some(quoted) => {
            let end = quoted.find('"')?;
            (&quoted[..end], quoted[end + 1..].trim())
        }
        None => match raw.rsplit_once(',') {
            Some((p, i)) if i.trim().parse::<i32>().is_ok() => (p, raw[p.len()..].trim()),
            _ => (raw, ""),
        },
    };
    let index = match rest.strip_prefix(',') {
        Some(i) => i.trim().parse().ok()?,
        None if rest.is_empty() => 0,
        None => return None,
    };
    let path = expand_env(path.trim());
    (!path.is_empty()).then_some((path, index))
}

/// Expands `%VAR%` with the process environment; unknown variables stay as they are.
fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(value) if !name.is_empty() => out.push_str(&value),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(windows)]
mod windows_icon {
    use std::ffi::c_void;
    use windows::core::{HRESULT, HSTRING};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
    };
    use windows::Win32::UI::Shell::SHDefExtractIconW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    /// Large icon size; the small one (high word) is not requested.
    const ICON_SIZE: u32 = 32;

    /// Renders the icon to 32-bit RGBA pixels.
    pub fn extract(path: &str, index: i32) -> Option<(u32, u32, Vec<u8>)> {
        let mut icon = HICON::default();
        // SAFETY: valid out pointer; the icon is destroyed below.
        let hr = unsafe {
            SHDefExtractIconW(
                &HSTRING::from(path),
                index,
                0,
                Some(&mut icon),
                None,
                ICON_SIZE,
            )
        };
        if hr != HRESULT(0) || icon.is_invalid() {
            return None;
        }
        let pixels = icon_pixels(icon);
        // SAFETY: we own the icon returned by SHDefExtractIconW.
        let _ = unsafe { DestroyIcon(icon) };
        pixels
    }

    fn icon_pixels(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
        let mut info = ICONINFO::default();
        // SAFETY: valid icon handle and out pointer; the bitmaps are deleted below.
        unsafe { GetIconInfo(icon, &mut info) }.ok()?;
        let result = if info.hbmColor.is_invalid() {
            None // Monochrome icon: too old to be worth showing.
        } else {
            color_pixels(info.hbmColor, info.hbmMask)
        };
        // SAFETY: GetIconInfo hands us copies of the bitmaps that we must free.
        unsafe {
            if !info.hbmColor.is_invalid() {
                let _ = DeleteObject(info.hbmColor.into());
            }
            if !info.hbmMask.is_invalid() {
                let _ = DeleteObject(info.hbmMask.into());
            }
        }
        result
    }

    fn color_pixels(color: HBITMAP, mask: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
        let mut bm = BITMAP::default();
        // SAFETY: the buffer is a BITMAP of the size we pass.
        let got = unsafe {
            GetObjectW(
                color.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut BITMAP as *mut c_void),
            )
        };
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        if got == 0 || !(1..=256).contains(&w) || !(1..=256).contains(&h) {
            return None;
        }
        // SAFETY: a memory DC compatible with the screen, deleted below.
        let dc = unsafe { CreateCompatibleDC(None) };
        let mut bgra = read_bits(dc, color, w, h);
        if let Some(px) = bgra.as_mut() {
            // Old icons have no alpha channel: take transparency from the AND mask.
            if px.chunks_exact(4).all(|p| p[3] == 0) {
                match read_bits(dc, mask, w, h) {
                    Some(m) => {
                        for (p, m) in px.chunks_exact_mut(4).zip(m.chunks_exact(4)) {
                            p[3] = if m[0] == 0 { 255 } else { 0 };
                        }
                    }
                    None => px.chunks_exact_mut(4).for_each(|p| p[3] = 255),
                }
            }
            px.chunks_exact_mut(4).for_each(|p| p.swap(0, 2)); // BGRA to RGBA
        }
        // SAFETY: created above.
        let _ = unsafe { DeleteDC(dc) };
        bgra.map(|px| (w as u32, h as u32, px))
    }

    /// Any 32-bit bitmap up to 512 px as RGBA pixels; opaque when it has no alpha channel.
    pub fn bitmap_pixels(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
        let mut bm = BITMAP::default();
        // SAFETY: the buffer is a BITMAP of the size we pass.
        let got = unsafe {
            GetObjectW(
                bitmap.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut BITMAP as *mut c_void),
            )
        };
        let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
        if got == 0 || !(1..=512).contains(&w) || !(1..=512).contains(&h) {
            return None;
        }
        // SAFETY: a memory DC compatible with the screen, deleted below.
        let dc = unsafe { CreateCompatibleDC(None) };
        let pixels = read_bits(dc, bitmap, w, h);
        // SAFETY: created above.
        let _ = unsafe { DeleteDC(dc) };
        let mut px = pixels?;
        if px.chunks_exact(4).all(|p| p[3] == 0) {
            px.chunks_exact_mut(4).for_each(|p| p[3] = 255);
        }
        px.chunks_exact_mut(4).for_each(|p| p.swap(0, 2)); // BGRA to RGBA
        Some((w as u32, h as u32, px))
    }

    /// The bitmap as top-down 32-bit BGRA rows.
    fn read_bits(dc: HDC, bitmap: HBITMAP, w: i32, h: i32) -> Option<Vec<u8>> {
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        // SAFETY: the buffer holds exactly h rows of w 32-bit pixels.
        let lines = unsafe {
            GetDIBits(
                dc,
                bitmap,
                0,
                h as u32,
                Some(buf.as_mut_ptr().cast()),
                &mut bmi,
                DIB_RGB_COLORS,
            )
        };
        (lines == h).then_some(buf)
    }
}

const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

pub fn png_data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64(png))
}

/// Encodes RGBA pixels as a PNG. Rows use the "Sub" filter, so flat and transparent areas
/// become runs of zeros that the run-length deflate below shrinks well.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let stride = width as usize * 4;
    let mut filtered = Vec::with_capacity((stride + 1) * height as usize);
    for row in rgba.chunks_exact(stride) {
        filtered.push(1); // Sub
        for (i, &b) in row.iter().enumerate() {
            filtered.push(if i < 4 { b } else { b.wrapping_sub(row[i - 4]) });
        }
    }
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, no interlace

    let mut png = PNG_SIGNATURE.to_vec();
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &zlib(&filtered));
    push_chunk(&mut png, b"IEND", &[]);
    png
}

fn push_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let crc = crc32(&png[start..]);
    png.extend_from_slice(&crc.to_be_bytes());
}

/// zlib stream with one fixed-Huffman deflate block that only encodes byte runs
/// (back-references at distance 1). Simple, and good enough for icons.
fn zlib(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.bits(1, 1); // final block
    w.bits(1, 2); // fixed Huffman codes
    let mut i = 0;
    while i < data.len() {
        let run = if i == 0 {
            0
        } else {
            data[i..]
                .iter()
                .take(258)
                .take_while(|&&b| b == data[i - 1])
                .count()
        };
        if run >= 3 {
            w.length(run);
            w.huffman(0, 5); // distance code 0 = distance 1
            i += run;
        } else {
            w.literal(u16::from(data[i]));
            i += 1;
        }
    }
    w.literal(256); // end of block
    let mut out = vec![0x78, 0x01];
    out.extend(w.finish());
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    used: u32,
}

impl BitWriter {
    /// Plain values (header, extra bits) are written least significant bit first.
    fn bits(&mut self, value: u32, count: u32) {
        self.acc |= value << self.used;
        self.used += count;
        while self.used >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.used -= 8;
        }
    }

    /// Huffman codes are written most significant bit first.
    fn huffman(&mut self, code: u32, len: u32) {
        self.bits(code.reverse_bits() >> (32 - len), len);
    }

    fn literal(&mut self, sym: u16) {
        let sym = u32::from(sym);
        match sym {
            0..=143 => self.huffman(0x30 + sym, 8),
            144..=255 => self.huffman(0x190 + sym - 144, 9),
            256..=279 => self.huffman(sym - 256, 7),
            _ => self.huffman(0xc0 + sym - 280, 8),
        }
    }

    fn length(&mut self, len: usize) {
        const BASE: [u16; 29] = [
            3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99,
            115, 131, 163, 195, 227, 258,
        ];
        const EXTRA: [u32; 29] = [
            0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
        ];
        let idx = BASE
            .iter()
            .rposition(|&b| usize::from(b) <= len)
            .unwrap_or(0);
        self.literal(257 + idx as u16);
        self.bits((len - usize::from(BASE[idx])) as u32, EXTRA[idx]);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.used > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(!0u32, |crc, &b| {
        (0..8).fold(crc ^ u32::from(b), |c, _| {
            if c & 1 == 1 {
                (c >> 1) ^ 0xedb8_8320
            } else {
                c >> 1
            }
        })
    })
}

fn adler32(data: &[u8]) -> u32 {
    let (a, b) = data.iter().fold((1u32, 0u32), |(a, b), &x| {
        let a = (a + u32::from(x)) % 65_521;
        (a, (b + a) % 65_521)
    });
    (b << 16) | a
}

fn base64(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }

    #[test]
    fn base64_pads_correctly() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn png_has_valid_structure_and_compresses_flat_areas() {
        let (w, h) = (32u32, 32u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for p in rgba.chunks_exact_mut(4).skip(300).take(200) {
            p.copy_from_slice(&[0x00, 0x67, 0xc0, 0xff]);
        }
        let png = encode_png(w, h, &rgba);
        assert!(png.starts_with(PNG_SIGNATURE));
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..24], &[0, 0, 0, 32, 0, 0, 0, 32]);
        assert!(png.ends_with(&[b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82]));
        // 4 KB of pixels; the flat image must shrink a lot.
        assert!(png.len() < 600, "png is {} bytes", png.len());
    }

    #[test]
    fn deflate_output_is_stable() {
        // Golden value checked against Python's zlib.decompress.
        let z = zlib(b"aaaaaaaaaabc");
        assert_eq!(z, [120, 1, 75, 132, 131, 164, 100, 0, 29, 158, 4, 144]);
    }

    #[test]
    fn parses_icon_locations() {
        assert_eq!(
            parse_icon_location(r#""C:\Program Files\App\app.exe",0"#),
            Some((r"C:\Program Files\App\app.exe".into(), 0))
        );
        assert_eq!(
            parse_icon_location(r"C:\Program Files\App\app.exe,-101"),
            Some((r"C:\Program Files\App\app.exe".into(), -101))
        );
        assert_eq!(
            parse_icon_location(r"C:\App\app.ico"),
            Some((r"C:\App\app.ico".into(), 0))
        );
        assert_eq!(
            parse_icon_location(r#""C:\App\app.exe""#),
            Some((r"C:\App\app.exe".into(), 0))
        );
        assert_eq!(
            parse_icon_location(r"C:\A,B\app.exe"),
            Some((r"C:\A,B\app.exe".into(), 0))
        );
        assert_eq!(parse_icon_location(""), None);
        assert_eq!(parse_icon_location(r#""C:\App\app.exe" junk"#), None);
    }

    #[test]
    fn expands_environment_variables() {
        std::env::set_var("EU_TEST_ROOT", r"C:\Windows");
        assert_eq!(expand_env(r"%EU_TEST_ROOT%\x.dll"), r"C:\Windows\x.dll");
        assert_eq!(expand_env("%EU_TEST_MISSING%\\x"), "%EU_TEST_MISSING%\\x");
        assert_eq!(expand_env("100% sure"), "100% sure");
    }
}
