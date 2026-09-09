//! `scan/platform` — per-OS walker/volume impls (SPEC §3.1/§3.10). macOS = dua-core 3.3.0.

use std::sync::Arc;

use crossbeam_channel::Sender;

use super::{DirStat, Kind, ScanEvent, ScanOptions, ScanStats, ScanTotals};
use crate::VolumeInfo;

#[cfg(target_os = "macos")]
pub mod macos;

pub fn start(
    opts: ScanOptions,
    tx: Sender<ScanEvent>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    stats: Arc<ScanStats>,
) -> std::thread::JoinHandle<ScanTotals> {
    #[cfg(target_os = "macos")]
    return macos::start(opts, tx, cancel, stats);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (opts, tx, cancel, stats);
        unimplemented!("portable baseline lands with M2 platform wiring")
    }
}

pub fn volumes() -> Vec<VolumeInfo> {
    #[cfg(target_os = "macos")]
    return macos::volumes();
    #[cfg(not(target_os = "macos"))]
    unimplemented!("portable baseline lands with M2 platform wiring")
}
