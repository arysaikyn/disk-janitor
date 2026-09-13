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

const KIB: u64 = 1024;
const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * 1024 * 1024;
const TIB: u64 = 1024 * 1024 * 1024 * 1024;

/// Recognized size units, matched as an exact lowercased token (so `gib` and
/// `gb` are separate aliases, and order is irrelevant). Binary multiples: the
/// `*iB` spellings are aliases of the short form, matching the product's
/// GiB-internal accounting. A unit token only counts when the character after
/// it is NOT ASCII-alphabetic — `25 goals` must not read as 25 GiB.
const UNITS: &[(&str, u64)] = &[
    ("kib", KIB),
    ("kb", KIB),
    ("k", KIB),
    ("mib", MIB),
    ("mb", MIB),
    ("m", MIB),
    ("gib", GIB),
    ("gb", GIB),
    ("g", GIB),
    ("tib", TIB),
    ("tb", TIB),
    ("t", TIB),
    ("b", 1),
];

/// Failure compiling a policy sentence. A named-but-unknown size unit is an
/// ERROR, never a default: silently dropping a byte target makes 「腾出 500 XB」
/// select every candidate (no cap) with a reassuring stderr line (Bug 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    /// The sentence named an alphabetic size unit the compiler does not know.
    UnrecognizedUnit(String),
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::UnrecognizedUnit(u) => write!(
                f,
                "unrecognized size unit {u:?} (expected b, kb/kib, mb/mib, gb/gib or tb/tib)"
            ),
        }
    }
}

impl std::error::Error for PolicyError {}

/// Ordered first-match rules:
///
/// 1. 「腾出 500 MB」 / "free up 50 GB" → `target_bytes` (first match wins). Units
///    accepted: b, kb/kib, mb/mib, gb/gib, tb/tib (binary multiples); a bare
///    single letter must end on a token boundary (so 「25 goals」 is not 25 GiB).
///    A number whose unit token is unknown is `Err` — dropping a byte target
///    silently removes the selection cap, which is the dangerous direction.
/// 2. 别动…两周 / 别动最近 (fragment-level, checked before the generic 别动 rule; the
///    protective verb 别动 is required — 「这两周」 alone is a description, not a
///    protection) → `deny_under_mta_secs = 14 d`.
/// 3. 别动 <target> (「别动 Xcode 归档」 expands to `~/Library/Developer/Xcode/Archives`;
///    otherwise the fragment tail, `~/` expanded) → `deny_globs +=` entry.
///
/// Rules 2/3 scan residual *fragments* — clause spans minus spans already consumed —
/// not just clause starts, so a space-glued 「腾出 25 GB 别动 Xcode 归档」 (target span
/// consumed, no punctuation) still protects its tail. Text no rule consumed lands in
/// `unmatched`; when nothing matches, `constraints` stays at defaults and
/// `applied_rules` is empty.
pub fn compile(sentence: &str) -> Result<CompiledPolicy, PolicyError> {
    let mut constraints = PolicyConstraintsShape::default();
    let mut applied_rules: Vec<&'static str> = Vec::new();
    let mut consumed: Vec<(usize, usize)> = Vec::new();

    // Rule 1: target bytes (verb-anchored, integer GB only in v1).
    if let Some((start, end, bytes)) = parse_target_bytes(sentence)? {
        constraints.target_bytes = Some(bytes);
        applied_rules.push(RULE_TARGET_BYTES);
        consumed.push((start, end));
    }

    let clauses = split_clauses(sentence);

    // Rule 2 (before Rule 3 so a 别动…两周 fragment is not also read as a glob):
    // deny_under_mta on residual fragments (clause spans minus spans already
    // consumed by Rule 1). Requires the protective verb 别动.
    for clause in &clauses {
        for (gs, ge) in clause_gaps(clause, &consumed) {
            if fragment_is_mta(&sentence[gs..ge]) {
                constraints.deny_under_mta_secs = TWO_WEEKS_SECS;
                applied_rules.push(RULE_DENY_UNDER_MTA);
                consumed.push((gs, ge));
            }
        }
    }

    // Rule 3: 别动 <target> → deny_globs. Scans residual fragments, not just clause
    // starts, so a space-glued 「腾出 25 GB 别动 Xcode 归档」 — target span consumed,
    // no punctuation — still protects its tail.
    for clause in &clauses {
        for (gs, ge) in clause_gaps(clause, &consumed) {
            let gap = &sentence[gs..ge];
            let Some(i) = gap.find("别动") else {
                continue;
            };
            let tail = gap[i + "别动".len()..].trim_start_matches([' ', '\u{3000}', '\t']);
            if tail.is_empty() {
                continue;
            }
            let glob = glob_for_tail(tail);
            if !constraints.deny_globs.contains(&glob) {
                constraints.deny_globs.push(glob);
                applied_rules.push(RULE_DENY_GLOBS);
            }
            consumed.push((gs, ge));
        }
    }

    Ok(CompiledPolicy {
        constraints,
        applied_rules,
        unmatched: unmatched_parts(sentence, &consumed),
    })
}

/// Compiled policy — `constraints` are the actionable result; `applied_rules` names
/// which rules fired; `unmatched` holds sentence parts no rule consumed.
#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    pub constraints: PolicyConstraintsShape,
    pub applied_rules: Vec<&'static str>,
    pub unmatched: Vec<String>,
}

struct Clause {
    start: usize,
    end: usize,
}

/// Split on CJK/ASCII clause delimiters; byte ranges are safe because delimiters are
/// single chars from `char_indices`.
fn split_clauses(s: &str) -> Vec<Clause> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, ch) in s.char_indices() {
        if matches!(ch, '，' | '。' | '；' | ';' | ',' | '、' | '\n') {
            out.push(Clause { start, end: i });
            start = i + ch.len_utf8();
        }
    }
    out.push(Clause {
        start,
        end: s.len(),
    });
    out
}

/// MTA protection requires the protective verb: 别动 plus a recent-time marker
/// (这两周 / 近两周 / 最近 — 别动最近 covers the last case). 「这两周」 alone is a
/// description, not a protection (D3).
fn fragment_is_mta(t: &str) -> bool {
    t.contains("别动") && (t.contains("这两周") || t.contains("近两周") || t.contains("最近"))
}

/// 「腾出|free up」 + spaces + number + spaces + unit → bytes.
/// First match wins (Hazel first-match). Returns consumed byte span + bytes.
///
/// `Err` when a verb is followed by a number whose unit token is a word the
/// table does not know (`25 goals`, `500 xb`): that is an authoring mistake, not
/// a default. `Ok(None)` when there is no verb, no number, or the number carries
/// no unit at all — free text stays soft (the NL compiler's normal mode).
fn parse_target_bytes(s: &str) -> Result<Option<(usize, usize, u64)>, PolicyError> {
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
        match parse_size(&s[vend..])? {
            Some((end, bytes)) => return Ok(Some((vstart, vend + end, bytes))),
            None => continue,
        }
    }
    Ok(None)
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

/// After the verb: `[ws]* digits [. digits] [ws]* unit` (case-insensitive).
/// `Ok(None)` = no number present. `Err` = number present but the following word
/// is not a known unit.
fn parse_size(rest: &str) -> Result<Option<(usize, u64)>, PolicyError> {
    let bytes = rest.as_bytes();
    let mut i = skip_ws(rest);
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return Ok(None); // verb without a number
    }
    // Absurd magnitude (u64 overflow) keeps the old soft path: `checked_mul`
    // below returns None → unmatched. Only an unknown UNIT is a hard error.
    let whole: u64 = rest[digits_start..i].parse().unwrap_or(u64::MAX);
    // Optional fractional part: 「0.5 GB」 is a size a user will type, and
    // rejecting it would put us back in the silently-uncapped state.
    let mut frac_num: u64 = 0;
    let mut frac_den: u64 = 1;
    if i < bytes.len() && bytes[i] == b'.' {
        let frac_start = i + 1;
        let mut j = frac_start;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j == frac_start {
            return Ok(None); // "25." with no fraction digits
        }
        let digits = j - frac_start;
        frac_num = rest[frac_start..j].parse().unwrap_or(0);
        // A digit run this long is far below one byte for every unit in UNITS
        // (the largest is TiB, so 1e-19 TiB < 1 B), so contribute 0 rather than
        // panicking on `10u64.pow`. The span is still consumed below, keeping
        // the matched/unmatched split unchanged.
        frac_den = 10u64
            .checked_pow(u32::try_from(digits).unwrap_or(u32::MAX))
            .unwrap_or(0);
        i = j;
    }
    i += skip_ws(&rest[i..]);
    let suffix = &rest[i..];
    let token_len = suffix.bytes().take_while(u8::is_ascii_alphabetic).count();
    if token_len == 0 {
        return Ok(None); // number without a unit
    }
    let token = suffix[..token_len].to_ascii_lowercase();
    let mult = UNITS
        .iter()
        .find(|(u, _)| *u == token)
        .map(|(_, m)| *m)
        .ok_or_else(|| PolicyError::UnrecognizedUnit(token.clone()))?;
    let whole_bytes = whole.checked_mul(mult);
    // `frac_den == 0` means the fraction digit run exceeded u64's power-of-ten range —
    // sub-byte by construction, so it contributes 0 (and never divides by zero).
    let frac_bytes = frac_num
        .saturating_mul(mult)
        .checked_div(frac_den)
        .unwrap_or(0);
    match whole_bytes {
        Some(w) => Ok(Some((i + token_len, w.saturating_add(frac_bytes)))),
        None => Ok(None), // absurd magnitude: fall through as unmatched, as before
    }
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

/// Residual fragments of `clause` not covered by any consumed rule span (consumed
/// spans are clamped to the clause range and merged). These gaps — not whole
/// clauses — are the units Rules 2/3 and `unmatched` operate on, so a rule that
/// consumes a span inside a clause (e.g. 腾出 25 GB) never hides the rest of the
/// clause from later rules.
fn clause_gaps(clause: &Clause, consumed: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut covered: Vec<(usize, usize)> = consumed
        .iter()
        .filter(|&&(s, e)| s < clause.end && clause.start < e)
        .map(|&(s, e)| (s.max(clause.start), e.min(clause.end)))
        .collect();
    covered.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in covered {
        match merged.last_mut() {
            Some((_, le)) if s <= *le => *le = (*le).max(e),
            _ => merged.push((s, e)),
        }
    }
    let mut out = Vec::new();
    let mut start = clause.start;
    for (s, e) in merged {
        if s > start {
            out.push((start, s));
        }
        start = start.max(e);
    }
    if start < clause.end {
        out.push((start, clause.end));
    }
    out
}

/// Text no rule consumed, one entry per residual fragment (trimmed, empty skipped).
fn unmatched_parts(s: &str, consumed: &[(usize, usize)]) -> Vec<String> {
    let mut out = Vec::new();
    for clause in split_clauses(s) {
        for (gs, ge) in clause_gaps(&clause, consumed) {
            let text = s[gs..ge].trim();
            if !text.is_empty() {
                out.push(text.to_string());
            }
        }
    }
    out
}
