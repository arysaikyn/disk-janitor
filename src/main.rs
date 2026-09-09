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
            let _ = min_bytes;
            janitor_scan::run_scan(roots)
        }
    }
}
pub mod janitor_scan {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use disk_janitor::scan::{ScanOptions, ScanStats};

    pub fn run_scan(_roots: Vec<std::path::PathBuf>) -> anyhow::Result<()> {
        let _opts = ScanOptions {
            roots: Vec::new(),
            threads: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4),
            cross_filesystems: false,
            ignore: Vec::new(),
            min_dir_bytes: 0,
        };
        let _cancel = Arc::new(AtomicBool::new(false));
        let _stats = Arc::new(ScanStats::default());
        unimplemented!("M1: scan --json pipeline (scan thread -> classify -> JSON out)")
    }
}
