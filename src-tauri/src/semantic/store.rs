//! The index on disk: for every photo and video looked at, its size, date and one
//! vector per picture (a photo has one, a video one per frame looked at). Vectors
//! are kept as 768 signed bytes each, so 10,000 photos take about 8 MB.
//!
//! The file is written to a temporary name and then renamed, so a crash never
//! leaves half an index behind.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// Numbers per vector (EmbeddingGemma 2's full size).
pub const DIM: usize = 768;
const MAGIC: &[u8; 4] = b"EUSI";
const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub size: u64,
    /// Seconds since 1970 of the last change: the file is looked at again when it differs.
    pub modified: u64,
    /// How deeply a video was looked at (`Depth as u8`); 0 for photos.
    pub depth: u8,
    /// `DIM` bytes per picture, one after the other. Empty when nothing could be read.
    pub vectors: Vec<i8>,
}

impl Entry {
    pub fn pictures(&self) -> usize {
        self.vectors.len() / DIM
    }

    /// How close the file is to `query` (unit length): the best of its pictures, from -1 to 1.
    pub fn score(&self, query: &[f32]) -> Option<f32> {
        self.vectors
            .as_chunks::<DIM>()
            .0
            .iter()
            .map(|v| v.iter().zip(query).map(|(a, b)| *a as f32 * b).sum::<f32>() / 127.0)
            .reduce(f32::max)
    }
}

#[derive(Debug, Default)]
pub struct Index {
    /// Which model made the vectors (`Quality::id`).
    pub model: String,
    pub entries: HashMap<PathBuf, Entry>,
}

/// Unit length, then each number as a byte from -127 to 127.
pub fn quantize(v: &[f32]) -> Option<Vec<i8>> {
    if v.len() < DIM || v.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let v = &v[..DIM];
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return None;
    }
    Some(
        v.iter()
            .map(|x| (x / norm * 127.0).round().clamp(-127.0, 127.0) as i8)
            .collect(),
    )
}

/// Unit length, cut to `DIM` numbers (EmbeddingGemma's vectors may be cut short and stay usable).
pub fn unit(v: &[f32]) -> Option<Vec<f32>> {
    if v.len() < DIM || v.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let v = &v[..DIM];
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    (norm > f32::EPSILON).then(|| v.iter().map(|x| x / norm).collect())
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

impl Index {
    pub fn new(model: &str) -> Self {
        Index {
            model: model.to_string(),
            entries: HashMap::new(),
        }
    }

    /// The saved index, or an empty one for `model` when there is none, it is
    /// damaged, or another model made it.
    pub fn load_or_new(path: &Path, model: &str) -> Self {
        match Self::load(path) {
            Ok(index) if index.model == model => index,
            _ => Self::new(model),
        }
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let mut r = BufReader::new(File::open(path)?);
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if &magic != MAGIC || read_u32(&mut r)? != VERSION {
            return Err(bad("not an index"));
        }
        let len = read_u32(&mut r)? as usize;
        if len > 256 {
            return Err(bad("model name"));
        }
        let mut model = vec![0u8; len];
        r.read_exact(&mut model)?;
        if read_u32(&mut r)? as usize != DIM {
            return Err(bad("vector size"));
        }
        let count = read_u64(&mut r)?;
        let mut entries = HashMap::new();
        for _ in 0..count {
            let len = read_u32(&mut r)? as usize;
            if len > 32_768 {
                return Err(bad("path"));
            }
            let mut path = vec![0u8; len];
            r.read_exact(&mut path)?;
            let path = String::from_utf8(path).map_err(|_| bad("path"))?;
            let size = read_u64(&mut r)?;
            let modified = read_u64(&mut r)?;
            let mut depth = [0u8; 1];
            r.read_exact(&mut depth)?;
            let pictures = read_u32(&mut r)? as usize;
            if pictures > 100_000 {
                return Err(bad("pictures"));
            }
            let mut bytes = vec![0u8; pictures * DIM];
            r.read_exact(&mut bytes)?;
            entries.insert(
                PathBuf::from(path),
                Entry {
                    size,
                    modified,
                    depth: depth[0],
                    vectors: bytes.into_iter().map(|b| b as i8).collect(),
                },
            );
        }
        Ok(Index {
            model: String::from_utf8(model).map_err(|_| bad("model name"))?,
            entries,
        })
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let tmp = path.with_extension("tmp");
        {
            let mut w = BufWriter::new(File::create(&tmp)?);
            w.write_all(MAGIC)?;
            w.write_all(&VERSION.to_le_bytes())?;
            w.write_all(&(self.model.len() as u32).to_le_bytes())?;
            w.write_all(self.model.as_bytes())?;
            w.write_all(&(DIM as u32).to_le_bytes())?;
            // Paths that are not valid Unicode cannot be written as text: they are left out.
            let entries: Vec<(&str, &Entry)> = self
                .entries
                .iter()
                .filter_map(|(p, e)| Some((p.to_str()?, e)))
                .collect();
            w.write_all(&(entries.len() as u64).to_le_bytes())?;
            for (path, e) in entries {
                w.write_all(&(path.len() as u32).to_le_bytes())?;
                w.write_all(path.as_bytes())?;
                w.write_all(&e.size.to_le_bytes())?;
                w.write_all(&e.modified.to_le_bytes())?;
                w.write_all(&[e.depth])?;
                w.write_all(&(e.pictures() as u32).to_le_bytes())?;
                let bytes: Vec<u8> = e.vectors[..e.pictures() * DIM]
                    .iter()
                    .map(|b| *b as u8)
                    .collect();
                w.write_all(&bytes)?;
            }
            w.flush()?;
        }
        fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(seed: usize) -> Vec<f32> {
        (0..DIM)
            .map(|i| (((i * 7 + seed * 13) % 23) as f32) - 11.0)
            .collect()
    }

    #[test]
    fn an_index_is_saved_and_read_back() {
        let dir = std::env::temp_dir().join(format!("eu-index-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("index.bin");
        let mut index = Index::new("m1");
        let mut two = quantize(&vector(1)).unwrap();
        two.extend(quantize(&vector(2)).unwrap());
        index.entries.insert(
            PathBuf::from("C:\\Users\\a\\Videos\\W20251201RM.mp4"),
            Entry {
                size: 10,
                modified: 20,
                depth: 2,
                vectors: two,
            },
        );
        index.entries.insert(
            PathBuf::from("C:\\Users\\a\\Pictures\\broken.jpg"),
            Entry {
                size: 1,
                modified: 2,
                depth: 0,
                vectors: Vec::new(),
            },
        );
        index.save(&file).unwrap();
        let back = Index::load(&file).unwrap();
        assert_eq!(back.model, "m1");
        assert_eq!(back.entries, index.entries);
        assert_eq!(
            Index::load_or_new(&file, "m2").entries.len(),
            0,
            "another model starts over"
        );
        fs::write(&file, b"EUSI garbage").unwrap();
        assert_eq!(Index::load_or_new(&file, "m1").entries.len(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_best_picture_of_a_file_counts() {
        let q = unit(&vector(2)).unwrap();
        let mut vectors = quantize(&vector(1)).unwrap();
        vectors.extend(quantize(&vector(2)).unwrap());
        let e = Entry {
            size: 0,
            modified: 0,
            depth: 2,
            vectors,
        };
        assert_eq!(e.pictures(), 2);
        let best = e.score(&q).unwrap();
        assert!(best > 0.99, "the second frame is the query itself: {best}");
        let empty = Entry {
            vectors: Vec::new(),
            ..e
        };
        assert_eq!(empty.score(&q), None);
        assert_eq!(quantize(&[0.0; DIM]), None);
        assert_eq!(quantize(&[1.0; 10]), None);
    }
}
