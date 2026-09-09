//! Config tests (Cluster E): partial-toml merge over defaults (E1), trailing-slash
//! endpoint trim (E2), and Debug secrecy (E3).
//!
//! `Config::load()` itself reads the REAL user config dir (`dirs::config_dir()`), so
//! it is never called here — the merge/trim surface is exercised through
//! `toml::from_str::<Config>` (the exact same path `load()` uses) and the public
//! `trim_endpoint` helper. `HttpChatClient::new`'s defensive trim is covered by the
//! stub-server test in http_client_tests.rs.

use disk_janitor::config::{Budget, Config, trim_endpoint};

/// E1: a partial config.toml (`model = "…"` only) parses, with every other field
/// falling back to `Config::default()` — no more "missing field endpoint" hard error.
#[test]
fn partial_toml_merges_over_defaults() {
    let raw = r#"model = "claude-3-5-sonnet""#;
    let cfg: Config = toml::from_str(raw).expect("partial toml parses");
    assert_eq!(cfg.model, "claude-3-5-sonnet");
    let def = Config::default();
    assert_eq!(cfg.endpoint, def.endpoint);
    assert_eq!(cfg.api_key, None);
    assert_eq!(cfg.ctx_len, def.ctx_len);
    assert_eq!(cfg.thinking, def.thinking);
    // Vec/struct fields compare via their serialized form (no PartialEq derives).
    assert_eq!(
        serde_json::to_value(&cfg.prices).unwrap(),
        serde_json::to_value(&def.prices).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&cfg.budget).unwrap(),
        serde_json::to_value(&Budget::default()).unwrap()
    );
}

/// E1: an empty config file is a valid merge — everything defaults.
#[test]
fn empty_toml_equals_default() {
    let cfg: Config = toml::from_str("").expect("empty toml parses");
    let def = Config::default();
    assert_eq!(cfg.endpoint, def.endpoint);
    assert_eq!(cfg.api_key, None);
    assert_eq!(cfg.model, def.model);
    assert_eq!(cfg.ctx_len, def.ctx_len);
    assert_eq!(
        serde_json::to_value(&cfg.prices).unwrap(),
        serde_json::to_value(&def.prices).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&cfg.budget).unwrap(),
        serde_json::to_value(&Budget::default()).unwrap()
    );
}

/// E1: a partial `[budget]` table also merges field-wise over Budget::default().
#[test]
fn partial_budget_table_merges() {
    let raw = "[budget]\nmax_turns = 3";
    let cfg: Config = toml::from_str(raw).expect("partial budget table parses");
    assert_eq!(cfg.budget.max_turns, 3);
    assert_eq!(cfg.budget.cost_usd, Budget::default().cost_usd);
    assert_eq!(cfg.budget.tokens, Budget::default().tokens);
}

/// E1: an invalid VALUE still errors (defaults fix missing fields, never wrong types).
#[test]
fn invalid_value_still_errors() {
    assert!(toml::from_str::<Config>(r#"ctx_len = "not-a-number""#).is_err());
    assert!(toml::from_str::<Config>("model = 42").is_err());
}

/// E3: `Debug` on Config never prints the API key — same redaction shape as redact().
#[test]
fn debug_never_prints_api_key() {
    let cfg = Config {
        api_key: Some("super-secret-xyz".into()),
        ..Config::default()
    };
    let dbg = format!("{cfg:?}");
    assert!(
        !dbg.contains("super-secret-xyz"),
        "Debug leaked the API key: {dbg}"
    );
    assert!(dbg.contains("***"), "expected redacted marker: {dbg}");
}

/// E3: with no key set, Debug says `none` (matches redact()).
#[test]
fn debug_shows_none_without_key() {
    let dbg = format!("{:?}", Config::default());
    assert!(dbg.contains("none"), "got: {dbg}");
}

/// E2: the endpoint normalizer strips trailing slashes (and leaves clean URLs alone).
#[test]
fn trim_endpoint_strips_trailing_slashes() {
    assert_eq!(
        trim_endpoint("http://127.0.0.1:9999/v1/"),
        "http://127.0.0.1:9999/v1"
    );
    assert_eq!(
        trim_endpoint("https://api.deepseek.com/v1"),
        "https://api.deepseek.com/v1"
    );
    assert_eq!(
        trim_endpoint("http://localhost:11434/v1/"),
        "http://localhost:11434/v1"
    );
    assert_eq!(trim_endpoint("http://x/"), "http://x");
    assert_eq!(trim_endpoint("http://x////"), "http://x");
}
