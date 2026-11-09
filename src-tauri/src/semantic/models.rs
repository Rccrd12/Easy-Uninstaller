//! What the local index downloads: EmbeddingGemma 2 (Google DeepMind, Apache 2.0)
//! as GGUF files from Unsloth, and llama.cpp's server for Windows to run it.
//! Every file is pinned by address and SHA-256: a file that does not match is
//! thrown away, never run.
//!
//! The model is two files: the text model, which also turns the pictures into
//! the shared vector space, and the vision part ("mmproj"), which reads them.

use serde::{Deserialize, Serialize};

pub struct Pinned {
    /// The name it is kept under, in the `models` folder.
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    /// The size in bytes, for the progress bar before the server says it.
    pub size: u64,
}

/// llama.cpp's own build for Windows on x64 processors, CPU only (works on any PC).
pub const ENGINE: Pinned = Pinned {
    file: "llama-b12207-bin-win-cpu-x64.zip",
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12207/llama-b12207-bin-win-cpu-x64.zip",
    sha256: "24447c546dca02bc449c7369eba07608cf5b170a59042ef388712654356d0d4c",
    size: 24_886_130,
};

/// The engine's version: a new one is unpacked next to the model when this changes.
pub const ENGINE_VERSION: &str = "b12207";

/// The files as they were at one revision of the repository, so they never change under the app.
macro_rules! hf {
    ($name:literal) => {
        concat!(
            "https://huggingface.co/unsloth/embeddinggemma-2-GGUF/resolve/412430755667d84b1ac64fe1fe2693aa6219f1d9/",
            $name
        )
    };
}

const TEXT_Q4: Pinned = Pinned {
    file: "embeddinggemma-2-UD-Q4_K_XL.gguf",
    url: hf!("embeddinggemma-2-UD-Q4_K_XL.gguf"),
    sha256: "ea905fd08e8061db77a0031cbb5096d459d7cfd2bdd0ad2ba66e483f16719493",
    size: 175_673_856,
};
const TEXT_Q8: Pinned = Pinned {
    file: "embeddinggemma-2-Q8_0.gguf",
    url: hf!("embeddinggemma-2-Q8_0.gguf"),
    sha256: "6f1bd4ac6c5df7444f9cca7ca36cafe6cfa34cd6f49fefb1e0b4be8143aed8bc",
    size: 309_855_520,
};
const TEXT_F16: Pinned = Pinned {
    file: "embeddinggemma-2-F16.gguf",
    url: hf!("embeddinggemma-2-F16.gguf"),
    sha256: "0d4350867eddbb94af485501a3d07e9374c47779d88f8a39cd1abd6a8e2d0901",
    size: 557_950_240,
};
const VISION_Q8: Pinned = Pinned {
    file: "mmproj-Q8_0.gguf",
    url: hf!("mmproj-Q8_0.gguf"),
    sha256: "90e7b0238009e2954f856f2081dcf7f35af026b64c765e98f4777053e1754460",
    size: 554_821_120,
};
const VISION_F16: Pinned = Pinned {
    file: "mmproj-F16.gguf",
    url: hf!("mmproj-F16.gguf"),
    sha256: "74bf65c860d292a1eebafd7ca85b44532a2528f03017f9204fe1695916d2c66f",
    size: 980_895_232,
};

/// How good (and how big) a model the user picked. Mirrors `IndexQuality` in types.ts.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    /// 4-bit text model: the smallest download, a little less precise.
    #[default]
    Light,
    /// 8-bit text model.
    Better,
    /// 16-bit text model and vision part: the most precise, three times the size.
    Best,
}

impl Quality {
    pub fn files(self) -> [&'static Pinned; 2] {
        match self {
            Quality::Light => [&TEXT_Q4, &VISION_Q8],
            Quality::Better => [&TEXT_Q8, &VISION_Q8],
            Quality::Best => [&TEXT_F16, &VISION_F16],
        }
    }

    /// Names the vectors in the index: another model means another index.
    pub fn id(self) -> &'static str {
        match self {
            Quality::Light => "embeddinggemma-2/q4/mmproj-q8",
            Quality::Better => "embeddinggemma-2/q8/mmproj-q8",
            Quality::Best => "embeddinggemma-2/f16/mmproj-f16",
        }
    }
}

/// Every model file any choice may use, to clean up the ones no longer needed.
pub fn all_model_files() -> [&'static str; 5] {
    [
        TEXT_Q4.file,
        TEXT_Q8.file,
        TEXT_F16.file,
        VISION_Q8.file,
        VISION_F16.file,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_is_pinned() {
        for q in [Quality::Light, Quality::Better, Quality::Best] {
            for f in q.files() {
                assert!(
                    f.url.contains("/resolve/412430755667"),
                    "{} comes from the pinned revision",
                    f.file
                );
                assert!(f.url.ends_with(f.file));
                assert_eq!(f.sha256.len(), 64);
                assert!(f.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
                assert!(f.size > 100_000_000);
            }
        }
        assert!(ENGINE.url.contains(ENGINE_VERSION));
        assert!(ENGINE.url.ends_with(ENGINE.file));
        assert_eq!(ENGINE.sha256.len(), 64);
    }
}
