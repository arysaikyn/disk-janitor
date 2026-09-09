//! `fixture-gen` — synthetic junk-shape trees for tests/demos (SPEC §7).
//! Builds node_modules+lockfile, orphan .venv, DerivedData shape, _cacache CAS,
//! pnpm hardlinks, sparse file, symlink chain+loop — all under the target root.
//! Never pointed at real user data.

use clap::Parser;

#[derive(Parser)]
struct Args {
    /// Root directory to build fixtures under (created; must be empty or new).
    root: std::path::PathBuf,
    /// Which fixture set to build (default: all).
    #[arg(long)]
    set: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    fixture_gen::build(&args.root, args.set.as_deref())?;
    println!("fixtures built under {}", args.root.display());
    Ok(())
}

pub mod fixture_gen {

    use std::path::Path;

    pub fn build(_root: &Path, _set: Option<&str>) -> anyhow::Result<()> {
        unimplemented!("M1: fixture tree builder (tested via integration tests)")
    }
}
