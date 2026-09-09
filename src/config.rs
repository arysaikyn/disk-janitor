//! `config/` — config.toml + env fallback (SPEC Part B §3.9, R3 fields + R6 budget).
//! Secrets never logged: `redact()` is the only safe debug form.

use serde::{Deserialize, Serialize};

/// Per-1M-token price row (D-20: model-keyed; DeepSeek hit/miss split = multiple rows).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPrice {
    pub model: String,
    pub prompt_miss_per_1m: f64,
    pub prompt_hit_per_1m: f64,
    pub completion_per_1m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    #[serde(default = "default_cost_usd")]
    pub cost_usd: f64,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    #[serde(default = "default_tokens")]
    pub tokens: u64,
}

fn default_cost_usd() -> f64 {
    1.0
}
fn default_max_turns() -> u32 {
    12
}
fn default_tokens() -> u64 {
    200_000
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            cost_usd: default_cost_usd(),
            max_turns: default_max_turns(),
            tokens: default_tokens(),
        }
    }
}

/// R3 config: endpoint, key, model, ctx len, thinking mode, prices, budget (§3.9).
/// Every field carries `#[serde(default)]` so a PARTIAL config.toml merges over
/// `Config::default()` instead of hard-erroring with "missing field" (E1).
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(skip_serializing, default)]
    pub api_key: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_ctx_len")]
    pub ctx_len: u32,
    #[serde(default)]
    pub thinking: bool,
    #[serde(default = "default_prices")]
    pub prices: Vec<ModelPrice>,
    #[serde(default)]
    pub budget: Budget,
}

fn default_endpoint() -> String {
    "https://api.deepseek.com/v1".into()
}
fn default_model() -> String {
    "deepseek-chat".into()
}
fn default_ctx_len() -> u32 {
    65_536
}
fn default_prices() -> Vec<ModelPrice> {
    vec![ModelPrice {
        model: "deepseek-chat".into(),
        prompt_miss_per_1m: 0.27,
        prompt_hit_per_1m: 0.07,
        completion_per_1m: 1.10,
    }]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: default_endpoint(),
            api_key: None,
            model: default_model(),
            ctx_len: default_ctx_len(),
            thinking: false,
            prices: default_prices(),
            budget: Budget::default(),
        }
    }
}

/// Manual Debug: the API key NEVER appears — same `***`/`none` shape as `redact()`
/// (E3; the old derive printed the raw key).
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("endpoint", &self.endpoint)
            .field(
                "api_key",
                &(if self.api_key.is_some() {
                    "***"
                } else {
                    "none"
                }),
            )
            .field("model", &self.model)
            .field("ctx_len", &self.ctx_len)
            .field("thinking", &self.thinking)
            .field("prices", &self.prices)
            .field("budget", &self.budget)
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config parse: {0}")]
    Parse(String),
    #[error("config io: {0}")]
    Io(#[from] std::io::Error),
}

/// Load `<config_dir>/disk-janitor/config.toml` (dirs::config_dir), env overrides
/// on top: DISK_JANITOR_ENDPOINT / _API_KEY / _MODEL / _BUDGET_USD.
pub fn load() -> Result<Config, ConfigError> {
    let mut cfg = match config_path() {
        Some(p) if p.exists() => {
            let raw = fs_err::read_to_string(&p)?;
            toml::from_str(&raw).map_err(|e| ConfigError::Parse(e.to_string()))?
        }
        _ => Config::default(),
    };
    if let Ok(ep) = std::env::var("DISK_JANITOR_ENDPOINT") {
        cfg.endpoint = ep;
    }
    if let Ok(k) = std::env::var("DISK_JANITOR_API_KEY") {
        cfg.api_key = Some(k);
    }
    if let Ok(m) = std::env::var("DISK_JANITOR_MODEL") {
        cfg.model = m;
    }
    if let Ok(b) = std::env::var("DISK_JANITOR_BUDGET_USD")
        && let Ok(v) = b.parse()
    {
        cfg.budget.cost_usd = v;
    }
    // E2: strip any trailing '/' so async-openai's unconditional `/chat/completions`
    // join never produces `//chat/completions` (applies to file, env, and default).
    cfg.endpoint = trim_endpoint(&cfg.endpoint);
    Ok(cfg)
}

/// Normalize a base URL: strip trailing '/' chars (E2). async-openai always appends
/// `/chat/completions` to the configured base, so `…/v1/` would 404 on
/// `//chat/completions`. `HttpChatClient::new` also calls this defensively, in case a
/// caller builds a `Config` by hand instead of through `load`.
pub fn trim_endpoint(endpoint: &str) -> String {
    endpoint.trim_end_matches('/').to_string()
}

pub fn config_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("disk-janitor").join("config.toml"))
}

impl Config {
    /// Safe debug/log form — the API key NEVER appears (§3.9).
    pub fn redact(&self) -> String {
        format!(
            "Config {{ endpoint: {}, api_key: {}, model: {}, ctx_len: {}, thinking: {}, prices: {}, budget: {:?} }}",
            self.endpoint,
            if self.api_key.is_some() {
                "***"
            } else {
                "none"
            },
            self.model,
            self.ctx_len,
            self.thinking,
            self.prices.len(),
            self.budget
        )
    }

    pub fn price_for(&self, model: &str) -> Option<&ModelPrice> {
        self.prices.iter().find(|p| p.model == model)
    }
}
