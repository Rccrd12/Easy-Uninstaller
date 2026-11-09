//! Similar photos: pictures that look almost the same without being identical
//! files, like a burst of shots, a retake, or a copy that was resized or sent
//! through a chat app. Read only, like the rest of the scan.
//!
//! No image decoder is shipped: each photo gets a small "print" from the preview
//! Windows already makes for File Explorer (the one the review shows too). The
//! print is a 128-bit difference hash of its brightness (which way the light
//! changes across a 9 by 8 grid, left to right and top to bottom) plus its
//! average color. Two photos are alike when both are close.
//!
//! Matching is strict on purpose: a missed pair costs a little space, a wrong
//! pair costs the user's attention. Screenshots and graphics (PNG, GIF, files
//! named like screenshots) are left out, because screens of the same app look
//! alike whatever they say, and so are nearly plain pictures.
//!
//! Prints are saved with each file's size and date (`photo-prints.tsv`), so a
//! later scan only looks at new or changed photos. Making prints has a time
//! limit per scan; photos left over are compared at the next scan.

use super::Seen;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Smaller pictures are icons, stickers and web images rather than photos.
pub const MIN_PHOTO_BYTES: u64 = 64 * 1024;
/// Photo formats. PNG, GIF and BMP are mostly screenshots and graphics; camera
/// raw files are left out because a raw file and its JPEG are kept on purpose.
const PHOTO_EXTENSIONS: [&str; 7] = ["jpg", "jpeg", "jpe", "heic", "heif", "webp", "avif"];
/// File names of screenshots (English and Italian Windows, Android, iPhone).
const SCREENSHOT_NAMES: [&str; 3] = ["screenshot", "schermata", "screen shot"];
/// How different two prints may be, in bits out of 128.
const MAX_DISTANCE: u32 = 12;
/// How far apart the average colors may be, per channel (0-255).
const MAX_COLOR_GAP: u8 = 16;
/// Pictures whose grid is flatter than this (0-255) are nearly plain: their
/// hash is mostly noise, so they are not compared.
const MIN_CONTRAST: u8 = 24;
/// How long one scan may spend making new prints.
pub const PRINT_BUDGET: Duration = Duration::from_secs(60);
/// At most this many photos are compared (the most recent ones).
const MAX_PHOTOS: usize = 40_000;
const MAX_CACHE_LINES: usize = 100_000;
pub const CACHE_FILE: &str = "photo-prints.tsv";

/// True for a photo worth comparing, by its name.
pub fn is_photo(path: &Path) -> bool {
    let ext_ok = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PHOTO_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    ext_ok && !SCREENSHOT_NAMES.iter().any(|s| name.starts_with(s))
}

/// What a photo looks like, in a few bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Print {
    /// Brightness rising left to right, one bit per cell of a 9 by 8 grid.
    rows: u64,
    /// Brightness rising top to bottom, one bit per cell of an 8 by 9 grid.
    cols: u64,
    /// Average red, green and blue.
    color: [u8; 3],
}

// Prints are made from Windows previews only (and in tests).
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
impl Print {
    /// The print of a picture given as RGBA pixels, row by row. `None` for a
    /// picture too small or too plain to tell apart from others.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Print> {
        let (w, h) = (width as usize, height as usize);
        if w < 9 || h < 9 || rgba.len() < w * h * 4 {
            return None;
        }
        let mut gray = Vec::with_capacity(w * h);
        let mut sum = [0u64; 3];
        for p in rgba[..w * h * 4].as_chunks::<4>().0 {
            let (r, g, b) = (u32::from(p[0]), u32::from(p[1]), u32::from(p[2]));
            sum[0] += u64::from(r);
            sum[1] += u64::from(g);
            sum[2] += u64::from(b);
            gray.push(((r * 299 + g * 587 + b * 114) / 1000) as u8);
        }
        let n = (w * h) as u64;
        let color = sum.map(|s| (s / n) as u8);

        let wide = shrink(&gray, w, h, 9, 8);
        let (lo, hi) = wide
            .iter()
            .fold((u8::MAX, 0), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        if hi - lo < MIN_CONTRAST {
            return None;
        }
        let mut rows = 0u64;
        for y in 0..8 {
            for x in 0..8 {
                rows = rows << 1 | u64::from(wide[y * 9 + x] < wide[y * 9 + x + 1]);
            }
        }
        let tall = shrink(&gray, w, h, 8, 9);
        let mut cols = 0u64;
        for y in 0..8 {
            for x in 0..8 {
                cols = cols << 1 | u64::from(tall[y * 8 + x] < tall[(y + 1) * 8 + x]);
            }
        }
        Some(Print { rows, cols, color })
    }

    /// Bits that differ, out of 128.
    fn distance(&self, other: &Print) -> u32 {
        (self.rows ^ other.rows).count_ones() + (self.cols ^ other.cols).count_ones()
    }

    fn color_gap(&self, other: &Print) -> u8 {
        (0..3)
            .map(|i| self.color[i].abs_diff(other.color[i]))
            .max()
            .unwrap_or(0)
    }

    pub fn looks_like(&self, other: &Print) -> bool {
        self.color_gap(other) <= MAX_COLOR_GAP && self.distance(other) <= MAX_DISTANCE
    }

    fn encode(&self) -> String {
        let [r, g, b] = self.color;
        format!("{:016x}{:016x}{r:02x}{g:02x}{b:02x}", self.rows, self.cols)
    }

    fn decode(s: &str) -> Option<Print> {
        if s.len() != 38 || !s.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
        Some(Print {
            rows: u64::from_str_radix(&s[..16], 16).ok()?,
            cols: u64::from_str_radix(&s[16..32], 16).ok()?,
            color: [byte(32)?, byte(34)?, byte(36)?],
        })
    }
}

/// Averages a `w` by `h` grayscale picture down to `cw` by `ch` cells.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn shrink(gray: &[u8], w: usize, h: usize, cw: usize, ch: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(cw * ch);
    for cy in 0..ch {
        let (y0, y1) = (cy * h / ch, ((cy + 1) * h / ch).max(cy * h / ch + 1));
        for cx in 0..cw {
            let (x0, x1) = (cx * w / cw, ((cx + 1) * w / cw).max(cx * w / cw + 1));
            let mut sum = 0u32;
            for y in y0..y1 {
                sum += gray[y * w + x0..y * w + x1]
                    .iter()
                    .map(|&v| u32::from(v))
                    .sum::<u32>();
            }
            out.push((sum / ((y1 - y0) * (x1 - x0)) as u32) as u8);
        }
    }
    out
}

/// Makes the print of one photo. On Windows, from the File Explorer preview.
pub type Printer = fn(&Path) -> Option<Print>;

/// Prints made by earlier scans, by path, with the size and date they were made for.
#[derive(Debug, Default)]
pub struct PrintCache {
    known: HashMap<PathBuf, Cached>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cached {
    size: u64,
    modified: u128,
    /// `None` when Windows had no preview, or the picture was too plain.
    print: Option<Print>,
}

fn nanos(t: SystemTime) -> u128 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos())
}

impl PrintCache {
    /// Reads the saved prints; an unreadable or damaged file just means starting over.
    pub fn load(file: &Path) -> Self {
        let text = fs::read_to_string(file).unwrap_or_default();
        let known = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(4, '\t');
                let size = parts.next()?.parse().ok()?;
                let modified = parts.next()?.parse().ok()?;
                let print = match parts.next()? {
                    "-" => None,
                    s => Some(Print::decode(s)?),
                };
                let path = PathBuf::from(parts.next()?);
                Some((
                    path,
                    Cached {
                        size,
                        modified,
                        print,
                    },
                ))
            })
            .collect();
        PrintCache { known }
    }

    /// Saves the prints of `keep` only, so photos that are gone are forgotten.
    /// Written to a temporary file first, so a crash never leaves half a list.
    pub fn save(&self, file: &Path, keep: &HashSet<PathBuf>) -> io::Result<()> {
        let mut text = String::new();
        let mut lines = 0;
        for (path, c) in &self.known {
            let Some(p) = path.to_str().filter(|_| keep.contains(path)) else {
                continue;
            };
            if lines == MAX_CACHE_LINES || p.contains(['\t', '\n', '\r']) {
                continue;
            }
            let print = c.print.map_or_else(|| "-".to_string(), |p| p.encode());
            text.push_str(&format!("{}\t{}\t{print}\t{p}\n", c.size, c.modified));
            lines += 1;
        }
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = file.with_extension("tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, file)
    }

    /// The saved print of a photo, if it was made for the file as it is now.
    fn get(&self, photo: &Seen) -> Option<Option<Print>> {
        self.known
            .get(&photo.path)
            .filter(|c| c.size == photo.size && c.modified == nanos(photo.modified))
            .map(|c| c.print)
    }

    fn put(&mut self, photo: &Seen, print: Option<Print>) {
        let cached = Cached {
            size: photo.size,
            modified: nanos(photo.modified),
            print,
        };
        self.known.insert(photo.path.clone(), cached);
    }
}

/// Finds sets of similar photos among `photos`. Each set has two photos or
/// more, the largest file first: it usually has the most detail, so it is the
/// one suggested to keep. Prints missing from `cache` are made with `printer`
/// on a few background threads for at most `budget`; a preview Windows takes
/// too long to make never holds up the scan.
pub fn find_similar(
    photos: &[Seen],
    cache: &mut PrintCache,
    printer: Printer,
    budget: Duration,
    tick: &mut dyn FnMut(),
) -> Vec<Vec<Seen>> {
    let mut photos: Vec<&Seen> = photos.iter().collect();
    // The most recent photos first: they are the ones still being sorted out.
    photos.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.path.cmp(&b.path)));
    photos.truncate(MAX_PHOTOS);

    let mut prints: Vec<Option<Print>> = Vec::with_capacity(photos.len());
    let mut missing = Vec::new();
    for (i, photo) in photos.iter().enumerate() {
        match cache.get(photo) {
            Some(print) => {
                tick();
                prints.push(print);
            }
            None => {
                missing.push(i);
                prints.push(None);
            }
        }
    }
    let paths: Vec<PathBuf> = missing.iter().map(|&i| photos[i].path.clone()).collect();
    for (j, print) in make_prints(paths, printer, budget, tick) {
        let i = missing[j];
        prints[i] = print;
        cache.put(photos[i], print);
    }

    let printed: Vec<(&Seen, Print)> = photos
        .iter()
        .zip(&prints)
        .filter_map(|(s, p)| p.map(|p| (*s, p)))
        .collect();
    group(&printed)
}

/// Makes prints on background threads until all are done or `budget` runs out.
/// Returns (index in `paths`, print) for every photo that was looked at.
fn make_prints(
    paths: Vec<PathBuf>,
    printer: Printer,
    budget: Duration,
    tick: &mut dyn FnMut(),
) -> Vec<(usize, Option<Print>)> {
    if paths.is_empty() {
        return Vec::new();
    }
    let deadline = Instant::now() + budget;
    let paths = Arc::new(paths);
    let next = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (send, receive) = mpsc::channel();
    let workers = std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .clamp(1, 4)
        .min(paths.len());
    for _ in 0..workers {
        let (paths, next, stop, send) = (paths.clone(), next.clone(), stop.clone(), send.clone());
        // Not joined: a preview that never comes back only keeps its own thread busy.
        std::thread::spawn(move || {
            let _com = platform::Com::init();
            while !stop.load(Ordering::Relaxed) {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = paths.get(i) else { break };
                if send.send((i, printer(path))).is_err() {
                    break;
                }
            }
        });
    }
    drop(send);
    let mut out = Vec::with_capacity(paths.len());
    while out.len() < paths.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        match receive.recv_timeout(left) {
            Ok(done) => {
                tick();
                out.push(done);
            }
            Err(_) => break,
        }
    }
    stop.store(true, Ordering::Relaxed);
    out
}

/// Puts photos that look alike in sets. Each set is built around one photo
/// (the first not in a set yet) with the photos that look like it, so a chain of
/// slightly different photos never adds up to a set of different ones.
fn group(printed: &[(&Seen, Print)]) -> Vec<Vec<Seen>> {
    let mut taken = vec![false; printed.len()];
    let mut sets = Vec::new();
    for i in 0..printed.len() {
        if taken[i] {
            continue;
        }
        let (_, print) = printed[i];
        let mut set = vec![i];
        for j in i + 1..printed.len() {
            if !taken[j] && print.looks_like(&printed[j].1) {
                set.push(j);
            }
        }
        if set.len() < 2 {
            continue;
        }
        for &j in &set {
            taken[j] = true;
        }
        let mut set: Vec<Seen> = set.into_iter().map(|j| printed[j].0.clone()).collect();
        set.sort_by(|a, b| {
            b.size
                .cmp(&a.size)
                .then(a.modified.cmp(&b.modified))
                .then(a.path.cmp(&b.path))
        });
        sets.push(set);
    }
    sets
}

#[cfg(windows)]
mod platform {
    use super::Print;
    use std::path::Path;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::DeleteObject;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY,
    };

    /// The preview size asked from Windows, in pixels.
    const PREVIEW_SIZE: i32 = 96;

    /// COM set up for the current thread while this lives.
    pub struct Com(bool);

    impl Com {
        pub fn init() -> Self {
            // SAFETY: undone in `drop` when it succeeded.
            Com(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
        }
    }

    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: matches the successful CoInitializeEx in `init`.
                unsafe { CoUninitialize() };
            }
        }
    }

    /// The print of the preview File Explorer shows for the photo. Only a real
    /// preview counts: never the file type's icon.
    pub fn print(path: &Path) -> Option<Print> {
        // SAFETY: plain COM calls on a thread with COM set up; the bitmap is ours
        // and deleted below.
        unsafe {
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok()?;
            let side = SIZE {
                cx: PREVIEW_SIZE,
                cy: PREVIEW_SIZE,
            };
            let bitmap = factory.GetImage(side, SIIGBF_THUMBNAILONLY).ok()?;
            let pixels = crate::apps::icon::bitmap_rgba(bitmap);
            let _ = DeleteObject(bitmap.into());
            let (w, h, rgba) = pixels?;
            Print::from_rgba(w, h, &rgba)
        }
    }
}

/// Easy Uninstaller targets Windows; elsewhere no previews are made.
#[cfg(not(windows))]
mod platform {
    use super::Print;
    use std::path::Path;

    pub struct Com;

    impl Com {
        pub fn init() -> Self {
            Com
        }
    }

    pub fn print(_path: &Path) -> Option<Print> {
        None
    }
}

/// The printer the app uses: Windows' own previews.
pub fn system_printer() -> Printer {
    platform::print
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up photo: a sky gradient, a sun, hills and a few objects, all
    /// placed by `seed`, drawn at any size so resized copies can be tried.
    fn scene(seed: u32, w: u32, h: u32, shift: f32) -> Vec<u8> {
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        let s = seed as f32;
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32 / w as f32 + shift, y as f32 / h as f32);
                let horizon = 0.55 + 0.12 * (fx * (3.0 + s % 5.0) + s).sin();
                let sun = ((fx - 0.2 - (s * 0.37) % 0.6).powi(2) + (fy - 0.25).powi(2)).sqrt();
                let tree = ((fx * (6.0 + s % 3.0) + s * 0.7).sin() > 0.85) && fy > horizon - 0.15;
                let (r, g, b) = if sun < 0.08 {
                    (255.0, 240.0, 180.0)
                } else if tree {
                    (40.0, 70.0, 35.0)
                } else if fy > horizon {
                    (60.0 + 40.0 * fy, 110.0 + (s * 13.0) % 60.0, 50.0)
                } else {
                    (
                        90.0 + (s * 29.0) % 80.0,
                        140.0 + 80.0 * fy,
                        220.0 - 60.0 * fy,
                    )
                };
                out.extend([r as u8, g as u8, b as u8, 255]);
            }
        }
        out
    }

    fn print_of(rgba: &[u8], w: u32, h: u32) -> Print {
        Print::from_rgba(w, h, rgba).expect("a scene has a print")
    }

    /// Small changes like a camera's noise or a different JPEG quality.
    fn noisy(rgba: &[u8], amount: i32, brighter: i32) -> Vec<u8> {
        let mut state = 12345u32;
        rgba.as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let mut out = [0u8; 4];
                for c in 0..3 {
                    state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    let n = (state >> 16) as i32 % (2 * amount + 1) - amount;
                    out[c] = (i32::from(p[c]) + n + brighter).clamp(0, 255) as u8;
                }
                out[3] = 255;
                out
            })
            .collect()
    }

    #[test]
    fn the_same_scene_matches_resized_noisy_or_slightly_moved() {
        let a = print_of(&scene(3, 96, 72, 0.0), 96, 72);
        let resized = print_of(&scene(3, 64, 48, 0.0), 64, 48);
        let noise = print_of(&noisy(&scene(3, 96, 72, 0.0), 6, 4), 96, 72);
        let moved = print_of(&scene(3, 96, 72, 0.01), 96, 72);
        for (name, other) in [("resized", resized), ("noisy", noise), ("moved", moved)] {
            assert!(
                a.looks_like(&other),
                "{name}: distance {}",
                a.distance(&other)
            );
        }
    }

    #[test]
    fn different_scenes_do_not_match() {
        let prints: Vec<Print> = (0..12)
            .map(|s| print_of(&scene(s * 7 + 1, 96, 72, 0.0), 96, 72))
            .collect();
        for i in 0..prints.len() {
            for j in i + 1..prints.len() {
                assert!(
                    !prints[i].looks_like(&prints[j]),
                    "scenes {i} and {j}: distance {}, color gap {}",
                    prints[i].distance(&prints[j]),
                    prints[i].color_gap(&prints[j])
                );
            }
        }
    }

    #[test]
    fn plain_or_tiny_pictures_have_no_print() {
        let gray = [128u8, 128, 128, 255].repeat(64 * 64);
        assert!(Print::from_rgba(64, 64, &gray).is_none());
        assert!(Print::from_rgba(8, 8, &scene(1, 8, 8, 0.0)).is_none());
        assert!(
            Print::from_rgba(64, 64, &[0u8; 10]).is_none(),
            "short buffer"
        );
    }

    #[test]
    fn same_layout_in_other_colors_does_not_match() {
        let a = scene(5, 96, 72, 0.0);
        let night: Vec<u8> = a
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0] / 3, p[1] / 3, p[2] / 2 + 40, 255])
            .collect();
        assert!(!print_of(&a, 96, 72).looks_like(&print_of(&night, 96, 72)));
    }

    #[test]
    fn prints_survive_saving() {
        let p = print_of(&scene(9, 96, 72, 0.0), 96, 72);
        assert_eq!(Print::decode(&p.encode()), Some(p));
        assert_eq!(Print::decode("zz"), None);
    }

    #[test]
    fn knows_which_files_are_photos() {
        let photo = |name: &str| is_photo(&Path::new("Pictures").join(name));
        assert!(photo("IMG_0001.JPG"));
        assert!(photo("holiday.heic"));
        assert!(!photo("Screenshot 2025-01-02.jpg"));
        assert!(!photo("Schermata 2025-01-02 alle 10.jpg"));
        assert!(!photo("diagram.png"));
        assert!(!photo("IMG_0001.CR2"));
        assert!(!photo("jpg"));
    }

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "easy-uninstaller-photos-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }

        /// A photo file whose name says which scene it shows: `"3-a.jpg"` is scene 3.
        fn photo(&self, name: &str, size: usize) -> Seen {
            let path = self.0.join(name);
            fs::write(&path, vec![1u8; size]).unwrap();
            Seen::from_meta(path.clone(), &fs::metadata(&path).unwrap())
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Prints the scene named by the file, slightly moved by its letter.
    fn fake_printer(path: &Path) -> Option<Print> {
        let name = path.file_stem()?.to_str()?;
        let (seed, letter) = name.split_once('-')?;
        if letter == "slow" {
            std::thread::sleep(Duration::from_secs(5));
        }
        let shift = f32::from(letter.as_bytes()[0] - b'a') * 0.004;
        Print::from_rgba(96, 72, &scene(seed.parse().ok()?, 96, 72, shift))
    }

    fn names(set: &[Seen]) -> Vec<String> {
        set.iter()
            .map(|s| s.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn finds_sets_largest_first_and_remembers_prints() {
        let d = Dir::new("sets");
        let photos = vec![
            d.photo("3-a.jpg", 100),
            d.photo("3-b.jpg", 300),
            d.photo("3-c.jpg", 200),
            d.photo("8-a.jpg", 100),
            d.photo("15-a.jpg", 100),
            d.photo("15-b.jpg", 150),
            d.photo("x-a.jpg", 100), // no preview
        ];
        let mut cache = PrintCache::default();
        let mut ticks = 0;
        let mut sets = find_similar(&photos, &mut cache, fake_printer, PRINT_BUDGET, &mut || {
            ticks += 1
        });
        sets.sort_by_key(|s| s.len());
        let sets: Vec<Vec<String>> = sets.iter().map(|s| names(s)).collect();
        assert_eq!(
            sets,
            [
                vec!["15-b.jpg", "15-a.jpg"],
                vec!["3-b.jpg", "3-c.jpg", "3-a.jpg"]
            ]
        );
        assert_eq!(ticks, photos.len());

        // Saved and read back: nothing needs a new print.
        let file = d.0.join("data").join(CACHE_FILE);
        let keep: HashSet<PathBuf> = photos.iter().map(|p| p.path.clone()).collect();
        cache.save(&file, &keep).unwrap();
        let mut again = PrintCache::load(&file);
        assert_eq!(again.known.len(), photos.len());
        let never = |_: &Path| -> Option<Print> { panic!("all prints are saved") };
        let sets = find_similar(&photos, &mut again, never, PRINT_BUDGET, &mut || {});
        assert_eq!(sets.len(), 2);

        // A changed photo gets a new print; a photo no longer there is forgotten.
        let changed = d.photo("8-a.jpg", 120);
        assert!(again.get(&changed).is_none());
        let keep: HashSet<PathBuf> = photos[..3].iter().map(|p| p.path.clone()).collect();
        again.save(&file, &keep).unwrap();
        assert_eq!(PrintCache::load(&file).known.len(), 3);
    }

    #[test]
    fn a_slow_preview_does_not_hold_up_the_scan() {
        let d = Dir::new("slow");
        let photos = vec![d.photo("3-a.jpg", 100), d.photo("3-slow.jpg", 100)];
        let mut cache = PrintCache::default();
        let start = Instant::now();
        let sets = find_similar(
            &photos,
            &mut cache,
            fake_printer,
            Duration::from_millis(300),
            &mut || {},
        );
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(sets.is_empty());
        assert_eq!(
            cache.known.len(),
            1,
            "the slow one is tried again next time"
        );
    }
}
