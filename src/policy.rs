//! `policy/` — intent → constraints (SPEC Part B §3.6). NL compiler v1: ordered
//! first-match keyword rules (Hazel's rule philosophy [02 §4]). Local rules only;
//! the LLM fallback (aggregates-only) is M4 `agent/` work.
//!
//! No `regex` crate is pinned (Cargo.toml frozen): the rules are hand-rolled
//! `str::find` + char scanning, CJK-spacing tolerant; glob *strings* are built here
//! and matched later by `globset` in `plan::build`.

use crate::plan::PolicyConstraintsShape;

/// Rule provenance tags recorded in `CompiledPolicy::applied_rules`.
pub const RULE_TARGET_BYTES: &str = "target_bytes";
pub const RULE_DENY_UNDER_MTA: &str = "deny_under_mta";
pub const RULE_DENY_GLOBS: &str = "deny_globs";

/// 「近两周 / 这两周 / 别动最近」 → 14 days in seconds.
pub const TWO_WEEKS_SECS: u64 = 14 * 24 * 60 * 60;

const GIB: u64 = 1024 * 1024 * 1024;

/// Ordered first-match rules:
///
/// 1. 「腾出 25GB」 / "free up 50 GB" → `target_bytes` (first match wins).
/// 2. 别动…两周 / 别动最近 (clause-level, checked before the generic 别动 rule) →
///    `deny_under_mta_secs = 14 d`.
/// 3. 别动 <target> (「别动 Xcode 归档」 expands to `~/Library/Developer/Xcode/Archives`;
///    otherwise the clause tail, `~/` expanded) → `deny_globs +=` entry.
///
/// Text no rule consumed lands in `unmatched`; when nothing matches, `constraints`
/// stays at defaults and `applied_rules` is empty.
pub fn compile(sentence: &str) -> CompiledPolicy {
    let mut constraints = PolicyConstraintsShape::default();
    let mut applied_rules: Vec<&'static str> = Vec::new();
    let mut consumed: Vec<(usize, usize)> = Vec::new();

    // Rule 1: target bytes (verb-anchored, integer GB only in v1).
    if let Some((start, end, bytes)) = parse_target_bytes(sentence) {
        constraints.target_bytes = Some(bytes);
        applied_rules.push(RULE_TARGET_BYTES);
        consumed.push((start, end));
    }

    // Rule 2 (before Rule 3 so a 别动…两周 clause is not also read as a glob):
    // clause-level deny_under_mta.
    for clause in split_clauses(sentence) {
        if clause_is_mta(clause.text) {
            constraints.deny_under_mta_secs = TWO_WEEKS_SECS;
            applied_rules.push(RULE_DENY_UNDER_MTA);
            consumed.push((clause.start, clause.end));
        }
    }

    // Rule 3: 别动 <target> → deny_globs (clauses not already consumed by Rule 2).
    for clause in split_clauses(sentence) {
        let trimmed = clause.text.trim();
        if !trimmed.starts_with("别动") {
            continue;
        }
        if consumed
            .iter()
            .any(|&(s, e)| s == clause.start && e == clause.end)
        {
            continue;
        }
        let tail = trimmed["别动".len()..].trim_start_matches([' ', '\u{3000}', '\t']);
        if tail.is_empty() {
            continue;
        }
        let glob = glob_for_tail(tail);
        if !constraints.deny_globs.contains(&glob) {
            constraints.deny_globs.push(glob);
            applied_rules.push(RULE_DENY_GLOBS);
        }
        consumed.push((clause.start, clause.end));
    }

    CompiledPolicy {
        constraints,
        applied_rules,
        unmatched: unmatched_parts(sentence, &consumed),
    }
}

/// Compiled policy — `constraints` are the actionable result; `applied_rules` names
/// which rules fired; `unmatched` holds sentence parts no rule consumed.
#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    pub constraints: PolicyConstraintsShape,
    pub applied_rules: Vec<&'static str>,
    pub unmatched: Vec<String>,
}

struct Clause<'a> {
    start: usize,
    end: usize,
    text: &'a str,
}

/// Split on CJK/ASCII clause delimiters; byte ranges are safe because delimiters are
/// single chars from `char_indices`.
fn split_clauses(s: &str) -> Vec<Clause<'_>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, ch) in s.char_indices() {
        if matches!(ch, '，' | '。' | '；' | ';' | ',' | '、' | '\n') {
            out.push(Clause {
                start,
                end: i,
                text: &s[start..i],
            });
            start = i + ch.len_utf8();
        }
    }
    out.push(Clause {
        start,
        end: s.len(),
        text: &s[start..],
    });
    out
}

fn clause_is_mta(t: &str) -> bool {
    t.contains("这两周")
        || t.contains("近两周")
        || t.contains("别动最近")
        || (t.contains("别动") && t.contains("最近"))
}

/// 「腾出|free up」 + spaces + digits + spaces + GB/G (case-insensitive) → bytes.
/// First match wins (Hazel first-match). Returns consumed byte span + bytes.
fn parse_target_bytes(s: &str) -> Option<(usize, usize, u64)> {
    let mut verbs: Vec<(usize, usize)> = Vec::new();
    if let Some(i) = s.find("腾出") {
        verbs.push((i, i + "腾出".len()));
    }
    let lower = s.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find("free up") {
        let i = from + rel;
        verbs.push((i, i + "free up".len()));
        from = i + 1;
    }
    for (vstart, vend) in verbs {
        if let Some((_, gb_end, bytes)) = parse_gb(&s[vend..]) {
            return Some((vstart, vend + gb_end, bytes));
        }
    }
    None
}

/// Byte offset of the first non-space char (ASCII space, tab, full-width space).
fn skip_ws(s: &str) -> usize {
    match s
        .char_indices()
        .find(|(_, c)| !matches!(c, ' ' | '\t' | '\u{3000}'))
    {
        Some((i, _)) => i,
        None => s.len(),
    }
}

/// After the verb: `[ \t\u{3000}]* digits [ \t\u{3000}]* (gb|g)` (case-insensitive).
fn parse_gb(rest: &str) -> Option<(usize, usize, u64)> {
    let mut i = skip_ws(rest);
    let digits_start = i;
    let bytes = rest.as_bytes();
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return None; // verb without a number
    }
    let digits_end = i;
    i += skip_ws(&rest[i..]);
    let suffix = rest[i..].to_ascii_lowercase();
    let suffix_len = if suffix.starts_with("gb") {
        2
    } else if suffix.starts_with('g') {
        1
    } else {
        return None; // number without a GB unit
    };
    let digits: u64 = rest[digits_start..digits_end].parse().ok()?;
    let gib_bytes = digits.checked_mul(GIB)?;
    Some((digits_start, i + suffix_len, gib_bytes))
}

/// Turn a 别动 clause tail into a glob string. 「Xcode 归档」 expands to the concrete
/// DerivedData-adjacent Archives root; a leading `~/` expands to the real home dir
/// (kept literal if home is unknown — such a glob simply matches nothing real).
fn glob_for_tail(tail: &str) -> String {
    let stripped: String = tail.chars().filter(|c| !c.is_whitespace()).collect();
    if stripped.contains("Xcode")
        && stripped.contains("归档")
        && let Some(home) = dirs::home_dir()
    {
        return home
            .join("Library/Developer/Xcode/Archives")
            .to_string_lossy()
            .into_owned();
    }
    let t = tail.trim();
    if let Some(rest) = t.strip_prefix("~/") {
        dirs::home_dir()
            .map(|h| h.join(rest).to_string_lossy().into_owned())
            .unwrap_or_else(|| t.to_string())
    } else if t == "~" {
        dirs::home_dir()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|| t.to_string())
    } else {
        t.to_string()
    }
}

/// Clauses whose byte range does not overlap any consumed rule span.
fn unmatched_parts(s: &str, consumed: &[(usize, usize)]) -> Vec<String> {
    split_clauses(s)
        .into_iter()
        .filter_map(|cl| {
            let text = cl.text.trim();
            if text.is_empty() {
                return None;
            }
            let overlapped = consumed.iter().any(|&(s, e)| cl.start < e && s < cl.end);
            (!overlapped).then(|| text.to_string())
        })
        .collect()
}
