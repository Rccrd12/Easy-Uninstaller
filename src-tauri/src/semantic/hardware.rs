//! What this PC offers the model, so that nobody has to choose: a graphics card
//! (then llama.cpp's Vulkan build runs the model on it, many times faster), and
//! which cores are the fast ones (on hybrid Intel processors, only the P-cores:
//! the E-cores and the second thread of each core slow it down).

use std::sync::OnceLock;

/// A graphics card with less memory than this (an integrated one) is not used.
const MIN_VIDEO_MEMORY: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hardware {
    /// A graphics card of its own, with enough memory for the model.
    pub gpu: bool,
    /// Threads for llama.cpp on the processor.
    pub threads: usize,
    /// The P-cores' logical processors, on hybrid processors.
    pub cpu_mask: Option<u64>,
}

/// Looked at once per run of the app.
pub fn get() -> &'static Hardware {
    static ONCE: OnceLock<Hardware> = OnceLock::new();
    ONCE.get_or_init(|| {
        let logical = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2);
        let (threads, cpu_mask) = choose_threads(&platform::cores(), logical);
        Hardware {
            gpu: platform::video_memory() >= MIN_VIDEO_MEMORY,
            threads,
            cpu_mask,
        }
    })
}

/// On a hybrid processor (cores of different efficiency classes), one thread per
/// P-core, kept on them; otherwise half the logical processors, as before (one per
/// core with Hyper-Threading), so the PC stays usable.
pub fn choose_threads(cores: &[(u8, u64)], logical: usize) -> (usize, Option<u64>) {
    let half = (logical / 2).max(1);
    let Some(best) = cores.iter().map(|c| c.0).max() else {
        return (half, None);
    };
    if cores.iter().all(|c| c.0 == best) {
        return (half, None);
    }
    let fast: Vec<&(u8, u64)> = cores.iter().filter(|c| c.0 == best).collect();
    let mask = fast.iter().fold(0u64, |m, c| m | c.1);
    (fast.len(), (mask != 0).then_some(mask))
}

/// The cores in what `GetLogicalProcessorInformationEx(RelationProcessorCore)` wrote:
/// each one's efficiency class and logical processors (group 0 only).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_cores(buf: &[u8]) -> Vec<(u8, u64)> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 42 <= buf.len() {
        let size =
            u32::from_le_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]]) as usize;
        if size < 42 || off + size > buf.len() {
            break;
        }
        let class = buf[off + 9];
        let mut mask = [0u8; 8];
        mask.copy_from_slice(&buf[off + 32..off + 40]);
        let group = u16::from_le_bytes([buf[off + 40], buf[off + 41]]);
        if group == 0 {
            out.push((class, u64::from_le_bytes(mask)));
        }
        off += size;
    }
    out
}

#[cfg(windows)]
mod platform {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };
    use windows::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, RelationProcessorCore,
        SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };

    /// The most memory of its own any graphics card has (0 without one).
    pub fn video_memory() -> u64 {
        // SAFETY: plain DXGI calls on objects owned here.
        unsafe {
            let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
                return 0;
            };
            let mut most = 0u64;
            let mut i = 0;
            while let Ok(adapter) = factory.EnumAdapters1(i) {
                i += 1;
                if let Ok(desc) = adapter.GetDesc1() {
                    let software = desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0;
                    if !software {
                        most = most.max(desc.DedicatedVideoMemory as u64);
                    }
                }
            }
            most
        }
    }

    pub fn cores() -> Vec<(u8, u64)> {
        let mut len = 0u32;
        // SAFETY: the first call only asks for the size.
        let _ = unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len) };
        if len == 0 || len > 1 << 20 {
            return Vec::new();
        }
        // u64s: the entries are 8-byte aligned.
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        // SAFETY: the buffer holds `len` bytes, as Windows asked.
        let filled = unsafe {
            GetLogicalProcessorInformationEx(
                RelationProcessorCore,
                Some(buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX),
                &mut len,
            )
        };
        if filled.is_err() {
            return Vec::new();
        }
        let bytes: Vec<u8> = buf.iter().flat_map(|w| w.to_le_bytes()).collect();
        super::parse_cores(&bytes[..len as usize])
    }
}

/// Easy Uninstaller targets Windows; elsewhere: no graphics card, cores unknown.
#[cfg(not(windows))]
mod platform {
    pub fn video_memory() -> u64 {
        0
    }

    pub fn cores() -> Vec<(u8, u64)> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(class: u8, mask: u64, group: u16) -> Vec<u8> {
        let mut e = vec![0u8; 48];
        e[4..8].copy_from_slice(&48u32.to_le_bytes());
        e[9] = class;
        e[30..32].copy_from_slice(&1u16.to_le_bytes());
        e[32..40].copy_from_slice(&mask.to_le_bytes());
        e[40..42].copy_from_slice(&group.to_le_bytes());
        e
    }

    #[test]
    fn only_the_p_cores_of_a_hybrid_processor() {
        // 6 P-cores with two threads each, 10 E-cores: like an Intel Core Ultra.
        let mut buf = Vec::new();
        for i in 0..6 {
            buf.extend(entry(1, 0b11 << (2 * i), 0));
        }
        for i in 0..10 {
            buf.extend(entry(0, 1 << (12 + i), 0));
        }
        buf.extend(entry(1, 1, 1)); // another group: left out
        let cores = parse_cores(&buf);
        assert_eq!(cores.len(), 16);
        assert_eq!(choose_threads(&cores, 22), (6, Some(0xFFF)));
    }

    #[test]
    fn half_the_threads_elsewhere() {
        let same: Vec<(u8, u64)> = (0..8).map(|i| (0, 0b11 << (2 * i))).collect();
        assert_eq!(choose_threads(&same, 16), (8, None), "8 cores, 16 threads");
        assert_eq!(choose_threads(&[], 4), (2, None), "unknown cores");
        assert_eq!(choose_threads(&[], 1), (1, None));
        assert!(parse_cores(&[0u8; 10]).is_empty(), "too short");
    }
}
