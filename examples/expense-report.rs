//! D-20 开销明细表 exporter — prints the cost rollup for a ledger file.
//!
//! Usage:
//!   cargo run --example expense-report -- <ledger.jsonl>   # specific ledger
//!   cargo run --example expense-report                     # default home ledger
//!
//! Rolls up `Kind::Budget` records via `disk_janitor::cost::{CostLedger, rows}`
//! and prints the stable D-20 table (日期/模型/调用/tokens/USD/CNY + TOTAL, see
//! `cost::render_table`) inside a fenced code block. `scripts/gen-expense.sh`
//! wraps this to regenerate `docs/expense-sheet.md` (M5).
//!
//! A missing ledger file is treated as "no spend recorded yet": the empty table
//! prints instead of an error, so a fresh install reports zero cost.

use std::path::PathBuf;

use disk_janitor::cost::{CostLedger, render_table};

fn main() -> anyhow::Result<()> {
    let path = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => disk_janitor::cost::default_ledger_path(),
    };

    let rows = if path.exists() {
        CostLedger::new(path.clone()).rows()?
    } else {
        eprintln!(
            "note: ledger {} absent — no spend recorded; printing the empty table",
            path.display()
        );
        Vec::new()
    };

    println!("```text");
    print!("{}", render_table(&rows));
    println!("```");
    Ok(())
}
