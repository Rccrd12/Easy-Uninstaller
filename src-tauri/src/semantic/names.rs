//! File names the model reads too, so that a search finds "Vacanze Sardegna 2023.jpg"
//! when the person writes "holiday": the model puts words of any language that
//! mean the same thing close together. Only names that say something are read:
//! "IMG_1234", "5E4181AF-BF27-…" and numbers are left out. When the file's own name
//! says nothing, its folder's may ("Vacanze 2023\IMG_1234.jpg").
//!
//! The vectors are kept in `names.bin`, in the same format as the pictures'.

use std::path::Path;

/// Names sent to the model in one request.
pub const BATCH: usize = 32;
/// At most this many names in the index.
pub const MAX_NAMES: usize = 300_000;
/// A name longer than this is cut: the start says what it is.
const MAX_CHARS: usize = 120;

/// Words cameras, phones and apps put in names: they say nothing about the file.
const NOISE: [&str; 24] = [
    "img",
    "image",
    "dsc",
    "dscn",
    "dscf",
    "dcim",
    "pxl",
    "vid",
    "video",
    "mvimg",
    "imag",
    "pic",
    "photo",
    "foto",
    "mov",
    "gopr",
    "gp",
    "screenshot",
    "copy",
    "copia",
    "new",
    "nuovo",
    "file",
    "untitled",
];

/// Folders whose name says where a file is, not what it is: their files get no
/// name from them.
const GENERIC_FOLDERS: [&str; 30] = [
    "desktop",
    "documents",
    "documenti",
    "downloads",
    "download",
    "scaricati",
    "pictures",
    "immagini",
    "videos",
    "video",
    "music",
    "musica",
    "camera roll",
    "rullino",
    "screenshots",
    "saved pictures",
    "whatsapp images",
    "whatsapp video",
    "telegram desktop",
    "artwork",
    "images",
    "img",
    "assets",
    "media",
    "photos",
    "foto",
    "onedrive",
    "temp",
    "new folder",
    "nuova cartella",
];

/// The words of `name`: letters and digits, any case.
fn words(name: &str) -> Vec<&str> {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect()
}

/// A word a person wrote: three letters or more, no digits (`W20251201RM` is a
/// code), not a camera's or app's word, not a block of a code in capitals (`BCAD`).
fn real_word(w: &str) -> bool {
    let letters = w.chars().filter(|c| c.is_alphabetic()).count();
    let digits = w.chars().filter(|c| c.is_ascii_digit()).count();
    let code = w.len() >= 4
        && w.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase());
    letters >= 3 && digits == 0 && !code && !NOISE.contains(&w.to_lowercase().as_str())
}

/// A block of a code: hex digits with at least one number (`5E4181AF`, `a08d`).
fn code_block(w: &str) -> bool {
    w.len() >= 4
        && w.chars().all(|c| c.is_ascii_hexdigit())
        && w.chars().any(|c| c.is_ascii_digit())
}

/// The name as the model reads it ("Vacanze_Sardegna-2023" → "Vacanze Sardegna 2023"),
/// when it holds a real word and is not a code (a GUID has several hex blocks).
fn readable(name: &str) -> Option<String> {
    let ws = words(name);
    if ws.iter().filter(|w| code_block(w)).count() >= 2 || !ws.iter().any(|w| real_word(w)) {
        return None;
    }
    Some(ws.join(" ").chars().take(MAX_CHARS).collect())
}

/// What the model reads for this file: its name without the extension, or, when
/// that says nothing, its folder's name. `None` when neither says anything.
pub fn text_of(path: &Path) -> Option<String> {
    let own = path.file_stem().and_then(|s| s.to_str()).and_then(readable);
    own.or_else(|| {
        path.parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .filter(|f| !GENERIC_FOLDERS.contains(&f.to_lowercase().as_str()))
            .and_then(readable)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_names_that_say_something_are_read() {
        let t = |p: &str| text_of(Path::new(p));
        assert_eq!(
            t("a/Vacanze_Sardegna-2023.jpg").as_deref(),
            Some("Vacanze Sardegna 2023")
        );
        assert_eq!(t("a/fattura_luce.pdf").as_deref(), Some("fattura luce"));
        assert_eq!(t("a/greeting-light.wav").as_deref(), Some("greeting light"));
        assert_eq!(
            t("Foto/Matrimonio Anna/IMG_1234.jpg").as_deref(),
            Some("Matrimonio Anna"),
            "the folder's"
        );
        assert_eq!(t("a/IMG_1234.jpg"), None);
        assert_eq!(t("a/DSC00042.JPG"), None);
        assert_eq!(t("a/PXL_20250612_101010.jpg"), None);
        assert_eq!(t("a/W20251201RM.mp4"), None);
        assert_eq!(
            t("artwork/5E4181AF-BF27-491B-A08D-84D05BD7E4C7_sk1.jpeg"),
            None
        );
        assert_eq!(t("a/2023-06-12 18.30.22.jpg"), None);
        assert_eq!(t("a/Screenshot 2025-03-01 101010.png"), None);
        assert_eq!(t("a/Café.jpg").as_deref(), Some("Café"), "any language");
        assert_eq!(t("a/decade.txt").as_deref(), Some("decade"));
        assert_eq!(
            t("a/0cde6542-8b6f-4020-8cdc-caff699a59.jpeg"),
            None,
            "a code in lower case"
        );
        assert_eq!(t("a/BCAD-EFAB.png"), None);
        assert_eq!(
            t("Downloads/IMG_1234.jpg"),
            None,
            "a folder that says only where"
        );
        let long = format!("a/{}.txt", "parola ".repeat(40));
        assert!(t(&long).unwrap().chars().count() <= MAX_CHARS);
    }
}
