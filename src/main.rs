//! `janitor` — thin CLI subset (ADR-14): scan --json / plan / restore / interactive purge.
//! Purge NEVER gains a --yes bypass (P4). Subcommands grow per milestone (M1: scan).

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "janitor",
    version,
    about = "还我磁盘 Giveback — disk janitor CLI"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scan roots and emit classified candidates as JSON (ADR-14).
    Scan {
        /// Roots to scan (default: home directory).
        roots: Vec<std::path::PathBuf>,
        /// Emit the raw JSON envelope (machine form).
        #[arg(long, default_value_t = true)]
        json: bool,
        /// Minimum candidate size in bytes.
        #[arg(long)]
        min_bytes: Option<u64>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Scan {
            roots,
            json: _,
            min_bytes,
        } => {
            let roots = if roots.is_empty() {
                vec![dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))?]
            } else {
                roots
            };
            janitor_scan::run_scan(roots, min_bytes)
        }
    }
}
pub mod janitor_scan {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use disk_janitor::Thresholds;
    use disk_janitor::classify;
    use disk_janitor::scan::{self, ScanEvent, ScanOptions, ScanStats};

    /// Scan pipeline (SPEC §3.1→§3.2): scan thread → channel → classify → JSON envelope
    /// to stdout. JSON shape: {"scan": ScanTotals, "candidates": [Candidate…]}.
    pub fn run_scan(roots: Vec<std::path::PathBuf>, min_bytes: Option<u64>) -> anyhow::Result<()> {
        let cancel = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(ScanStats::default());
        let opts = ScanOptions {
            roots: roots.clone(),
            threads: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4),
            cross_filesystems: false,
            ignore: Vec::new(),
            min_dir_bytes: 0,
        };
        let (tx, rx) = crossbeam_channel::bounded::<ScanEvent>(100);
        let handle = scan::start(opts, tx, cancel, stats.clone());
        let mut batches: Vec<disk_janitor::scan::DirStat> = Vec::new();
        let mut totals = None;
        for event in rx {
            match event {
                ScanEvent::Batch(b) => batches.extend(b),
                ScanEvent::RootError { root, err } => {
                    eprintln!("scan error at {}: {err}", root.display());
                }
                ScanEvent::Finished(t) => {
                    totals = Some(t);
                    break;
                }
            }
        }
        let scan_totals = totals.ok_or_else(|| anyhow::anyhow!("scan ended without totals"))?;
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("scan thread panicked"))?;
        let th = Thresholds {
            min_bytes: min_bytes.unwrap_or(Thresholds::default().min_bytes),
            ..Thresholds::default()
        };
        let candidates = classify::candidates_from_batch(batches, &th);
        let envelope = serde_json::json!({ "scan": scan_totals, "candidates": candidates });
        serde_json::to_writer_pretty(std::io::stdout().lock(), &envelope)?;
        println!();
        Ok(())
    }
}
