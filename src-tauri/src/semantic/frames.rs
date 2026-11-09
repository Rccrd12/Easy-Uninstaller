//! The pictures the model looks at: a photo's preview, and frames of a video.
//!
//! Previews come from Windows (the same ones Explorer shows). Frames of a video
//! are read with Media Foundation, which ships with Windows; where it is missing
//! (Windows "N" editions) or cannot read the file, the video's preview is used.

use super::Depth;
use crate::search::FileKind;
use std::path::Path;

/// The longer side of a picture sent to the model, in pixels.
pub const SIDE: u32 = 448;
/// "Partial": this many frames, spread over the video.
pub const PARTIAL_FRAMES: u64 = 8;
/// "Full": one frame every this many seconds...
pub const FULL_EVERY_SECS: u64 = 5;
/// ...but never more than this many (a 2-hour film); longer videos spread them out.
pub const FULL_MAX_FRAMES: u64 = 1440;

/// When to take frames, in 100-nanosecond units (Media Foundation's), for a video this long.
pub fn frame_times(duration: u64, depth: Depth) -> Vec<u64> {
    let secs = duration / 10_000_000;
    let count = match depth {
        Depth::Quick => 1,
        Depth::Partial => PARTIAL_FRAMES,
        Depth::Full => (secs / FULL_EVERY_SECS).clamp(1, FULL_MAX_FRAMES),
    };
    if duration == 0 {
        return vec![0];
    }
    // The middle of each equal slice: never the black first frame, never past the end.
    (0..count)
        .map(|i| duration * (2 * i + 1) / (2 * count))
        .collect()
}

/// The pictures of one file, as PNGs. Empty when nothing could be read.
pub fn pictures(path: &Path, kind: FileKind, depth: Depth) -> Vec<Vec<u8>> {
    let preview = || {
        crate::review::thumbnail_png(path, SIDE)
            .into_iter()
            .collect::<Vec<_>>()
    };
    match kind {
        FileKind::Video if depth != Depth::Quick => {
            let frames = platform::video_frames(path, &|d| frame_times(d, depth));
            if frames.is_empty() {
                preview()
            } else {
                frames
            }
        }
        _ => preview(),
    }
}

/// `width` x `height` pixels, 4 bytes each in B, G, R, X order, `stride` bytes per row
/// (negative: the rows go bottom-up), shrunk to fit `SIDE` and made RGBA.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn bgrx_to_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    stride: i32,
) -> Option<(u32, u32, Vec<u8>)> {
    if width == 0 || height == 0 || stride.unsigned_abs() < width * 4 {
        return None;
    }
    let row_len = stride.unsigned_abs() as usize;
    if data.len() < row_len * (height as usize - 1) + width as usize * 4 {
        return None;
    }
    let scale = (SIDE as f32 / width.max(height) as f32).min(1.0);
    let (w, h) = (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    );
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let sy = ((y as f32 + 0.5) / scale) as u32;
        let sy = sy.min(height - 1) as usize;
        let row = if stride < 0 {
            height as usize - 1 - sy
        } else {
            sy
        };
        let base = row * row_len;
        for x in 0..w {
            let sx = (((x as f32 + 0.5) / scale) as u32).min(width - 1) as usize;
            let p = base + sx * 4;
            out.extend_from_slice(&[data[p + 2], data[p + 1], data[p], 255]);
        }
    }
    Some((w, h, out))
}

#[cfg(windows)]
mod platform {
    use super::bgrx_to_rgba;
    use std::path::Path;
    use windows::core::{GUID, HSTRING};
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    use windows::Win32::System::Variant::VT_I8;

    /// Media Foundation's "all streams" for stream selection.
    const ALL_STREAMS: u32 = 0xFFFF_FFFE;

    pub fn video_frames(path: &Path, times: &dyn Fn(u64) -> Vec<u64>) -> Vec<Vec<u8>> {
        // SAFETY: COM and Media Foundation are set up for this call and undone below.
        let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let started = unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE) }.is_ok();
        let frames = if started {
            // SAFETY: plain Media Foundation calls on objects owned here.
            unsafe { read(path, times) }.unwrap_or_default()
        } else {
            Vec::new()
        };
        if started {
            // SAFETY: matches the successful MFStartup above.
            let _ = unsafe { MFShutdown() };
        }
        if init.is_ok() {
            // SAFETY: matches the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
        }
        frames
    }

    unsafe fn read(
        path: &Path,
        times: &dyn Fn(u64) -> Vec<u64>,
    ) -> windows::core::Result<Vec<Vec<u8>>> {
        unsafe {
            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 1)?;
            let attrs = attrs.ok_or_else(windows::core::Error::empty)?;
            // Lets the reader turn any video into plain RGB pictures.
            attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
            let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), &attrs)?;
            let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            let _ = reader.SetStreamSelection(ALL_STREAMS, false);
            reader.SetStreamSelection(stream, true)?;
            let wanted = MFCreateMediaType()?;
            wanted.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            wanted.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            reader.SetCurrentMediaType(stream, None, &wanted)?;
            let current = reader.GetCurrentMediaType(stream)?;
            let size = current.GetUINT64(&MF_MT_FRAME_SIZE)?;
            let (width, height) = ((size >> 32) as u32, size as u32);
            let stride = current
                .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                .map(|s| s as i32)
                .unwrap_or(width as i32 * 4);

            let mut duration = reader
                .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)?;
            let length = duration.Anonymous.Anonymous.Anonymous.uhVal;
            let _ = PropVariantClear(&mut duration);

            let mut out = Vec::new();
            for t in times(length) {
                let mut at = PROPVARIANT::default();
                (*at.Anonymous.Anonymous).vt = VT_I8;
                (*at.Anonymous.Anonymous).Anonymous.hVal = t as i64;
                // A position the file cannot seek to: the next frame is taken.
                let _ = reader.SetCurrentPosition(&GUID::zeroed(), &at);
                // The first frame after the seek: the key frame nearest to `t`.
                let mut flags = 0u32;
                let mut sample: Option<IMFSample> = None;
                for _ in 0..8 {
                    reader.ReadSample(
                        stream,
                        0,
                        None,
                        Some(&mut flags),
                        None,
                        Some(&mut sample),
                    )?;
                    if sample.is_some() || flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                        break;
                    }
                }
                let Some(sample) = sample else { break };
                let buffer = sample.ConvertToContiguousBuffer()?;
                let mut data = std::ptr::null_mut();
                let mut len = 0u32;
                buffer.Lock(&mut data, None, Some(&mut len))?;
                let bytes = std::slice::from_raw_parts(data, len as usize);
                let picture = bgrx_to_rgba(bytes, width, height, stride);
                let _ = buffer.Unlock();
                if let Some((w, h, rgba)) = picture {
                    out.push(crate::apps::icon::encode_png(w, h, &rgba));
                }
            }
            Ok(out)
        }
    }
}

/// Easy Uninstaller targets Windows; elsewhere videos give no frames (the preview is used).
#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    pub fn video_frames(_path: &Path, _times: &dyn Fn(u64) -> Vec<u64>) -> Vec<Vec<u8>> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: u64 = 10_000_000;

    #[test]
    fn frames_are_spread_over_the_video() {
        assert_eq!(frame_times(60 * SEC, Depth::Quick), [30 * SEC]);
        let partial = frame_times(80 * SEC, Depth::Partial);
        assert_eq!(partial.len(), 8);
        assert_eq!(partial[0], 5 * SEC);
        assert_eq!(partial[7], 75 * SEC);
        assert_eq!(
            frame_times(60 * SEC, Depth::Full).len(),
            12,
            "one every 5 seconds"
        );
        assert_eq!(
            frame_times(3 * 3600 * SEC, Depth::Full).len(),
            1440,
            "a long film is capped"
        );
        assert_eq!(frame_times(2 * SEC, Depth::Full).len(), 1);
        assert_eq!(frame_times(0, Depth::Full), [0]);
    }

    #[test]
    fn frames_become_small_rgba_pictures() {
        // 2x2, bottom-up: the first row in memory is the bottom one.
        let data = [
            1, 2, 3, 0, 4, 5, 6, 0, // bottom
            7, 8, 9, 0, 10, 11, 12, 0, // top
        ];
        let (w, h, rgba) = bgrx_to_rgba(&data, 2, 2, -8).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(
            &rgba[..4],
            &[9, 8, 7, 255],
            "top left, colors swapped to RGB"
        );
        let big = vec![0u8; 1920 * 1080 * 4];
        let (w, h, rgba) = bgrx_to_rgba(&big, 1920, 1080, 1920 * 4).unwrap();
        assert_eq!((w, h), (SIDE, 252));
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert!(
            bgrx_to_rgba(&data, 2, 2, 4).is_none(),
            "a stride shorter than a row"
        );
        assert!(bgrx_to_rgba(&data[..10], 2, 2, 8).is_none());
    }
}
