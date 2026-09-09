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
    pub cost_usd: f64,
    pub max_turns: u32,
    pub tokens: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            cost_usd: 1.0,
            max_turns: 12,
            tokens: 200_000,
        }
    }
}

/// R3 config: endpoint, key, model, ctx len, thinking mode, prices, budget (§3.9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub endpoint: String,
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
    pub model: String,
    pub ctx_len: u32,
    pub thinking: bool,
    pub prices: Vec<ModelPrice>,
    pub budget: Budget,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: "https://api.deepseek.com/v1".into(),
            api_key: None,
            model: "deepseek-chat".into(),
            ctx_len: 65_536,
            thinking: false,
            prices: vec![ModelPrice {
                model: "deepseek-chat".into(),
                prompt_miss_per_1m: 0.27,
                prompt_hit_per_1m: 0.07,
                completion_per_1m: 1.10,
            }],
            budget: Budget::default(),
        }
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
    Ok(cfg)
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
