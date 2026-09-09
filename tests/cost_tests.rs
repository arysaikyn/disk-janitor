//! Integration tests for `cost/` — R6 token & cost accounting (SPEC Part B §3.7, D-20).
//!
//! Budget records are written through the real `ledger::Store::append` path (the same
//! path the agent loop uses) and, for the two-date grouping case, hand-written with
//! controlled timestamps in the frozen v1 envelope format (crc32fast over the canonical
//! `rec` bytes + seq as little-endian — byte-identical to `ledger::crc_of`).

use std::path::Path;

use disk_janitor::cost::{
    CostRow, USD_CNY, UsageLike, effective_usage, estimate_unverified, render_table, rows,
    total_usd,
};
use disk_janitor::ledger::{self, Kind, LedgerRecord};
use tempfile::TempDir;

/// One envelope line in the frozen ledger format v1, with a caller-controlled timestamp.
/// CRC is computed exactly like the writer's: `crc32fast(serde_json::to_vec(rec) ++
/// seq.to_le_bytes())`.
fn budget_line(
    seq: u64,
    ts: &str,
    model: &str,
    calls: u32,
    prompt_tokens: u64,
    completion_tokens: u64,
    cost_usd: f64,
) -> String {
    let rec = LedgerRecord::Budget {
        model: model.to_string(),
        calls,
        prompt_tokens,
        completion_tokens,
        cost_usd,
    };
    let rec_bytes = serde_json::to_vec(&rec).expect("serialize rec");
    let mut crc_input = rec_bytes.clone();
    crc_input.extend_from_slice(&seq.to_le_bytes());
    let crc32 = crc32fast::hash(&crc_input);
    let env = serde_json::json!({
        "v": 1,
        "seq": seq,
        "ts": ts,
        "kind": "budget",
        "crc32": crc32,
        "prev_crc": null,
        "rec": rec,
    });
    serde_json::to_string(&env).expect("serialize envelope") + "\n"
}

fn write_ledger(home: &Path, lines: &str) {
    std::fs::create_dir_all(home).expect("create home");
    std::fs::write(home.join("ledger.jsonl"), lines).expect("write ledger");
}

fn assert_close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-12, "expected {a} ≈ {b} within 1e-12");
}

#[test]
fn rows_groups_budget_recs_by_date_and_model() {
    let home = TempDir::new().expect("tempdir");
    // 3 budget recs: 2 models × 2 dates (dates controlled via hand-written envelopes).
    write_ledger(
        home.path(),
        &format!(
            "{}{}{}",
            budget_line(
                1,
                "2026-09-08T12:00:00Z",
                "deepseek-chat",
                2,
                1000,
                300,
                0.0012
            ),
            budget_line(
                2,
                "2026-09-08T14:30:00Z",
                "deepseek-chat",
                1,
                500,
                100,
                0.0004
            ),
            budget_line(
                3,
                "2026-09-09T09:00:00Z",
                "gpt-4o-mini",
                1,
                300,
                90,
                0.000456
            ),
        ),
    );
    let path = home.path().join("ledger.jsonl");

    let rows = rows(&path).expect("rows");

    assert_eq!(rows.len(), 2, "one row per (date, model)");
    // BTreeMap key order: date asc, then model.
    assert_eq!(
        rows[0].date,
        chrono::NaiveDate::from_ymd_opt(2026, 9, 8).unwrap()
    );
    assert_eq!(rows[0].model, "deepseek-chat");
    assert_eq!(rows[0].calls, 3, "calls sum across same (date, model)");
    assert_eq!(rows[0].prompt_tokens, 1500);
    assert_eq!(rows[0].completion_tokens, 400);
    assert_close(rows[0].cost_usd, 0.0016);

    assert_eq!(
        rows[1].date,
        chrono::NaiveDate::from_ymd_opt(2026, 9, 9).unwrap()
    );
    assert_eq!(rows[1].model, "gpt-4o-mini");
    assert_eq!(rows[1].calls, 1);
    assert_eq!(rows[1].prompt_tokens, 300);
    assert_eq!(rows[1].completion_tokens, 90);
    assert_close(rows[1].cost_usd, 0.000456);
}

#[test]
fn rows_reads_records_appended_through_the_store() {
    let home = TempDir::new().expect("tempdir");
    let mut store = ledger::open(home.path()).expect("open store");
    // A non-budget record must be ignored by the rollup.
    store
        .append(
            Kind::Scan,
            &LedgerRecord::Scan {
                root: [0u8; 8],
                files_scanned: 10,
                total_bytes: 100,
                duration_ms: 5,
                session_id: "s1".to_string(),
            },
        )
        .expect("append scan");
    store
        .append(
            Kind::Budget,
            &LedgerRecord::Budget {
                model: "deepseek-chat".to_string(),
                calls: 2,
                prompt_tokens: 1000,
                completion_tokens: 300,
                cost_usd: 0.0012,
            },
        )
        .expect("append budget");
    store
        .append(
            Kind::Budget,
            &LedgerRecord::Budget {
                model: "deepseek-chat".to_string(),
                calls: 1,
                prompt_tokens: 500,
                completion_tokens: 100,
                cost_usd: 0.0004,
            },
        )
        .expect("append budget");
    drop(store);

    let path = home.path().join("ledger.jsonl");
    let rows = rows(&path).expect("rows");

    assert_eq!(rows.len(), 1, "scan rec ignored; budget recs grouped");
    let r = &rows[0];
    assert_eq!(r.model, "deepseek-chat");
    assert_eq!(r.calls, 3);
    assert_eq!(r.prompt_tokens, 1500);
    assert_eq!(r.completion_tokens, 400);
    assert_close(r.cost_usd, 0.0016);
    // Store stamps Utc::now() — expect today's date.
    assert_eq!(r.date, chrono::Utc::now().date_naive());
}

#[test]
fn render_table_has_d20_headers_and_total() {
    let home = TempDir::new().expect("tempdir");
    write_ledger(
        home.path(),
        &format!(
            "{}{}",
            budget_line(
                1,
                "2026-09-08T12:00:00Z",
                "deepseek-chat",
                2,
                1000,
                300,
                0.0012
            ),
            budget_line(
                2,
                "2026-09-09T09:00:00Z",
                "gpt-4o-mini",
                1,
                300,
                90,
                0.000456
            ),
        ),
    );
    let path = home.path().join("ledger.jsonl");
    let rows = rows(&path).expect("rows");
    let table = render_table(&rows);

    for header in [
        "日期 date",
        "模型 model",
        "调用 calls",
        "tokens 提示+补全",
        "USD",
        "CNY",
        "TOTAL",
    ] {
        assert!(
            table.contains(header),
            "missing header {header:?} in:\n{table}"
        );
    }
    // Data cells present.
    assert!(table.contains("2026-09-08"));
    assert!(table.contains("deepseek-chat"));
    assert!(table.contains("1500/400") || table.contains("1000/300"));
    // TOTAL row carries summed tokens and money.
    let total_line = table
        .lines()
        .find(|l| l.contains("TOTAL"))
        .expect("total line");
    assert!(total_line.contains("3"), "TOTAL calls sum: {total_line}");
    assert!(
        total_line.contains("1300/390"),
        "TOTAL tokens sum: {total_line}"
    );
}

#[test]
fn total_usd_sums_all_budget_recs() {
    let home = TempDir::new().expect("tempdir");
    write_ledger(
        home.path(),
        &format!(
            "{}{}{}",
            budget_line(
                1,
                "2026-09-08T12:00:00Z",
                "deepseek-chat",
                2,
                1000,
                300,
                0.0012
            ),
            budget_line(
                2,
                "2026-09-08T14:30:00Z",
                "deepseek-chat",
                1,
                500,
                100,
                0.0004
            ),
            budget_line(
                3,
                "2026-09-09T09:00:00Z",
                "gpt-4o-mini",
                1,
                300,
                90,
                0.000456
            ),
        ),
    );
    let path = home.path().join("ledger.jsonl");
    assert_close(total_usd(&path).expect("total"), 0.0012 + 0.0004 + 0.000456);
}

#[test]
fn empty_ledger_yields_empty_rows_and_zero_total() {
    let home = TempDir::new().expect("tempdir");
    // ledger::open creates an empty ledger.jsonl — no records at all.
    let store = ledger::open(home.path()).expect("open store");
    drop(store);

    let path = home.path().join("ledger.jsonl");
    assert!(rows(&path).expect("rows").is_empty());
    assert_eq!(total_usd(&path).expect("total"), 0.0);
}

#[test]
fn cny_is_usd_times_static_rate() {
    let home = TempDir::new().expect("tempdir");
    write_ledger(
        home.path(),
        &budget_line(
            1,
            "2026-09-08T12:00:00Z",
            "deepseek-chat",
            1,
            100,
            20,
            0.001234,
        ),
    );
    let path = home.path().join("ledger.jsonl");
    let rows = rows(&path).expect("rows");
    assert_eq!(rows.len(), 1);
    // cost_cny is computed as cost_usd × USD_CNY in the same float domain — exact.
    assert_close(rows[0].cost_cny, rows[0].cost_usd * USD_CNY);
    assert_close(rows[0].cost_cny, 0.001234 * 7.2);
}

#[test]
fn effective_usage_splits_cached_prompt_tokens() {
    let u = UsageLike {
        prompt_tokens: 1000,
        cached_tokens: 400,
        completion_tokens: 200,
    };
    assert_eq!(effective_usage(&u), (600, 400, 200));
    // Misreporting provider: cached > prompt → clamped, never negative miss tokens.
    let u2 = UsageLike {
        prompt_tokens: 100,
        cached_tokens: 500,
        completion_tokens: 50,
    };
    assert_eq!(effective_usage(&u2), (0, 100, 50));
}

#[test]
fn estimate_unverified_is_chars_over_four() {
    // "hello world" (11) + "hi" (2) = 13 chars → 3.
    assert_eq!(estimate_unverified("hello world", "hi"), 3);
    assert_eq!(estimate_unverified("", ""), 0);
}

#[test]
fn cost_row_is_public_shape() {
    // Pin the D-20 row shape so the M5 开销明细表 consumer can rely on it.
    let row = CostRow {
        date: chrono::NaiveDate::from_ymd_opt(2026, 9, 8).unwrap(),
        model: "deepseek-chat".to_string(),
        calls: 3,
        prompt_tokens: 1500,
        completion_tokens: 400,
        cost_usd: 0.0016,
        cost_cny: 0.01152,
    };
    assert_eq!(row.calls, 3);
    assert_eq!(row.prompt_tokens, 1500);
    assert_close(row.cost_cny, row.cost_usd * USD_CNY);
}
