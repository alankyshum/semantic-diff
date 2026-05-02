use crate::llm_cli::{default_provider_order, parse_provider_order, LlmProvider};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Environment variable that overrides the LLM provider order.
/// Deprecated in favor of the `--llm-providers` CLI flag and config file.
pub const ENV_LLM_PROVIDERS: &str = "SEMANTIC_DIFF_LLM_PROVIDERS";

/// User configuration. Loaded from `~/.config/semantic-diff.json` (JSONC supported).
///
/// Precedence (highest wins):
/// 1. CLI flag (applied via [`Config::apply_overrides`])
/// 2. `SEMANTIC_DIFF_LLM_PROVIDERS` env var (deprecated; logs a warning)
/// 3. Config file
/// 4. Built-in defaults
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Config {
    pub preferred_ai_cli: Option<AiCli>,
    pub claude_model: String,
    pub copilot_model: String,
    pub cursor_model: String,
    pub llm_providers: Vec<LlmProvider>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiCli {
    Claude,
    Copilot,
}

/// Raw on-disk JSON shape. Stable kebab/nested layout; used for both load and save.
///
/// Note: `save()` writes plain JSON; any hand-edited comments in the existing
/// JSONC file will be lost on save.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[schemars(title = "SemanticDiffConfig")]
#[serde(default, deny_unknown_fields)]
pub struct RawConfig {
    #[serde(rename = "preferred-ai-cli", skip_serializing_if = "Option::is_none")]
    pub preferred_ai_cli: Option<AiCli>,
    #[serde(rename = "llm-providers", skip_serializing_if = "Option::is_none")]
    pub llm_providers: Option<Vec<LlmProvider>>,
    #[serde(skip_serializing_if = "CliConfig::is_empty")]
    pub claude: CliConfig,
    #[serde(skip_serializing_if = "CliConfig::is_empty")]
    pub copilot: CliConfig,
    #[serde(skip_serializing_if = "CliConfig::is_empty")]
    pub cursor: CliConfig,
}

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CliConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl CliConfig {
    fn is_empty(&self) -> bool {
        self.model.is_none()
    }
}

impl RawConfig {
    /// Generate the JSON Schema for the on-disk config shape.
    pub fn json_schema_value() -> serde_json::Value {
        let schema = schemars::schema_for!(RawConfig);
        let mut value = serde_json::to_value(&schema).expect("schema serializes");
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "$id".to_string(),
                serde_json::Value::String(
                    "https://raw.githubusercontent.com/alankyshum/semantic-diff/main/schemas/semantic-diff.schema.json"
                        .to_string(),
                ),
            );
        }
        value
    }
}

/// Model tier for intelligent cross-backend mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ModelTier {
    Fast,
    Balanced,
    Power,
}

impl Config {
    pub fn default_config() -> Self {
        Self {
            preferred_ai_cli: None,
            claude_model: "haiku".to_string(),
            copilot_model: "gemini-flash".to_string(),
            cursor_model: "auto".to_string(),
            llm_providers: default_provider_order(),
        }
    }

    /// Apply runtime overrides on top of an already-loaded config.
    ///
    /// Precedence: `providers_override` (CLI) > `SEMANTIC_DIFF_LLM_PROVIDERS` env >
    /// existing value (from file or default).
    pub fn apply_overrides(&mut self, providers_override: Option<Vec<LlmProvider>>) {
        if let Some(p) = providers_override {
            if !p.is_empty() {
                self.llm_providers = p;
                return;
            }
        }
        if let Ok(raw) = std::env::var(ENV_LLM_PROVIDERS) {
            tracing::warn!(
                "{ENV_LLM_PROVIDERS} is deprecated; prefer --llm-providers or the config file"
            );
            match parse_provider_order(&raw) {
                Ok(p) => self.llm_providers = p,
                Err(e) => tracing::warn!("ignoring invalid {ENV_LLM_PROVIDERS}: {e}"),
            }
        }
    }

    /// Convert in-memory config into on-disk shape.
    fn to_raw(&self) -> RawConfig {
        RawConfig {
            preferred_ai_cli: self.preferred_ai_cli,
            llm_providers: Some(self.llm_providers.clone()),
            claude: CliConfig { model: Some(self.claude_model.clone()) },
            copilot: CliConfig { model: Some(self.copilot_model.clone()) },
            cursor: CliConfig { model: Some(self.cursor_model.clone()) },
        }
    }

    /// Atomically write config to `config_path()` (tempfile + rename).
    ///
    /// Writes plain JSON — any hand-edited JSONC comments in the existing file
    /// are lost. Atomic on the same filesystem.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path()
            .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        save_to(self, &path)
    }
}

fn save_to(config: &Config, path: &Path) -> anyhow::Result<()> {
    let raw = config.to_raw();
    save_raw(&raw, path)
}

/// Atomically write a hand-edited [`RawConfig`] to `path` (tempfile + rename).
///
/// Mirrors the atomic-write pattern used by [`Config::save`]. Used by the
/// settings UI's `PUT /api/config` endpoint to round-trip the on-disk shape
/// directly without going through the in-memory `Config` projection (which
/// would otherwise normalize/lose fields).
///
/// Writes plain JSON; any JSONC comments in the existing file are lost.
pub fn save_raw(raw: &RawConfig, path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(raw)?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    use std::io::Write;
    tmp.write_all(body.as_bytes())?;
    tmp.write_all(b"\n")?;
    tmp.flush()?;
    tmp.persist(path).map_err(|e| anyhow::anyhow!("failed to persist config: {}", e.error))?;
    Ok(())
}

/// Config file path: ~/.config/semantic-diff.json
pub fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| config_path_for_load(&h))
}

/// Test/inject hook: derive the config path from a given home directory.
pub fn config_path_for_load(home: &Path) -> PathBuf {
    home.join(".config").join("semantic-diff.json")
}

const DEFAULT_CONFIG: &str = r#"{
  // Which AI CLI to prefer: "claude" or "copilot"
  // Falls back to the other if the preferred one is not installed.
  // If unset, defaults to: claude > copilot
  // "preferred-ai-cli": "claude",

  // LLM provider fallback order. Array of "claude" | "copilot" | "cursor".
  // "llm-providers": ["claude", "copilot", "cursor"],

  // Claude CLI settings
  "claude": {
    // Model: "haiku" (fast, default), "sonnet" (balanced), "opus" (powerful)
    "model": "haiku"
  },

  // Copilot CLI settings
  "copilot": {
    // Model: "gemini-flash" (fast, default), "sonnet", "opus", "haiku", "gemini-pro"
    "model": "gemini-flash"
  },

  // Cursor CLI settings
  "cursor": {
    "model": "auto"
  }
}
"#;

/// Load config from disk. Creates a default commented config if none exists.
pub fn load() -> Config {
    let path = match config_path() {
        Some(p) => p,
        None => {
            tracing::warn!("Could not determine home directory, using default config");
            return Config::default_config();
        }
    };
    load_from(&path)
}

fn load_from(path: &Path) -> Config {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, DEFAULT_CONFIG);
        tracing::info!("Created default config at {}", path.display());
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to read config {}: {}", path.display(), e);
            return Config::default_config();
        }
    };

    let stripped = strip_json_comments(&content);
    let raw: RawConfig = match serde_json::from_str(&stripped) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Failed to parse config {}: {}", path.display(), e);
            return Config::default_config();
        }
    };

    let llm_providers = raw.llm_providers.unwrap_or_else(default_provider_order);
    Config {
        preferred_ai_cli: raw.preferred_ai_cli,
        claude_model: resolve_model_for_claude(raw.claude.model.as_deref()),
        copilot_model: resolve_model_for_copilot(raw.copilot.model.as_deref()),
        cursor_model: raw.cursor.model.unwrap_or_else(|| "auto".to_string()),
        llm_providers,
    }
}

fn resolve_model_for_claude(model: Option<&str>) -> String {
    let tier = model.map(model_tier).unwrap_or(ModelTier::Fast);
    match tier {
        ModelTier::Fast => "haiku",
        ModelTier::Balanced => "sonnet",
        ModelTier::Power => "opus",
    }
    .to_string()
}

fn resolve_model_for_copilot(model: Option<&str>) -> String {
    match model {
        Some(m) => {
            let tier = model_tier(m);
            match m {
                "sonnet" | "opus" | "haiku" | "gemini-flash" | "gemini-pro" => m.to_string(),
                _ => match tier {
                    ModelTier::Fast => "gemini-flash",
                    ModelTier::Balanced => "sonnet",
                    ModelTier::Power => "opus",
                }
                .to_string(),
            }
        }
        None => "gemini-flash".to_string(),
    }
}

fn model_tier(name: &str) -> ModelTier {
    let n = name.to_lowercase();
    if n.contains("flash") || n.contains("haiku") || n == "gpt-4o-mini" || n.ends_with("-mini") {
        ModelTier::Fast
    } else if n.contains("opus") {
        ModelTier::Power
    } else {
        ModelTier::Balanced
    }
}

/// Strip // and /* */ comments from JSON text (simple JSONC support).
pub fn strip_json_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' => match chars.peek() {
                Some('/') => {
                    for rest in chars.by_ref() {
                        if rest == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next();
                    let mut prev = ' ';
                    for rest in chars.by_ref() {
                        if prev == '*' && rest == '/' {
                            break;
                        }
                        if rest == '\n' {
                            out.push('\n');
                        }
                        prev = rest;
                    }
                }
                _ => out.push(c),
            },
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_tier_mapping() {
        assert_eq!(model_tier("haiku"), ModelTier::Fast);
        assert_eq!(model_tier("gemini-flash"), ModelTier::Fast);
        assert_eq!(model_tier("sonnet"), ModelTier::Balanced);
        assert_eq!(model_tier("opus"), ModelTier::Power);
    }

    #[test]
    fn test_default_config_parses() {
        let stripped = strip_json_comments(DEFAULT_CONFIG);
        let raw: RawConfig = serde_json::from_str(&stripped).unwrap();
        assert!(raw.preferred_ai_cli.is_none());
        assert_eq!(raw.claude.model.as_deref(), Some("haiku"));
        assert_eq!(raw.copilot.model.as_deref(), Some("gemini-flash"));
    }

    #[test]
    fn test_save_raw_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cfg.json");
        let raw = RawConfig {
            preferred_ai_cli: Some(AiCli::Claude),
            llm_providers: Some(vec![LlmProvider::Cursor, LlmProvider::Claude]),
            claude: CliConfig { model: Some("sonnet".into()) },
            copilot: CliConfig { model: Some("opus".into()) },
            cursor: CliConfig { model: Some("auto".into()) },
        };
        save_raw(&raw, &path).unwrap();
        let txt = std::fs::read_to_string(&path).unwrap();
        let back: RawConfig = serde_json::from_str(&txt).unwrap();
        assert_eq!(back.preferred_ai_cli, Some(AiCli::Claude));
        assert_eq!(back.llm_providers, Some(vec![LlmProvider::Cursor, LlmProvider::Claude]));
        assert_eq!(back.claude.model.as_deref(), Some("sonnet"));
        assert_eq!(back.copilot.model.as_deref(), Some("opus"));
        assert_eq!(back.cursor.model.as_deref(), Some("auto"));
    }

    #[test]
    fn test_save_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = config_path_for_load(tmp.path());
        let mut cfg = Config::default_config();
        cfg.cursor_model = "fast".into();
        save_to(&cfg, &path).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.cursor_model, "fast");
        assert_eq!(loaded.llm_providers, cfg.llm_providers);
    }

    /// Precedence: CLI override > env > file > default.
    #[test]
    fn test_precedence_cli_beats_env_beats_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = config_path_for_load(tmp.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // File pins copilot-only.
        std::fs::write(
            &path,
            r#"{ "llm-providers": ["copilot"], "claude": {"model": "haiku"}, "copilot": {"model": "gemini-flash"} }"#,
        )
        .unwrap();

        // 1. File-only: should be ["copilot"].
        let cfg = load_from(&path);
        assert_eq!(cfg.llm_providers, vec![LlmProvider::Copilot]);

        // 2. Env beats file. Use a guard since env is process-global.
        let _guard = EnvGuard::set(ENV_LLM_PROVIDERS, "cursor");
        let mut cfg = load_from(&path);
        cfg.apply_overrides(None);
        assert_eq!(cfg.llm_providers, vec![LlmProvider::Cursor]);

        // 3. CLI beats env.
        let mut cfg = load_from(&path);
        cfg.apply_overrides(Some(vec![LlmProvider::Claude]));
        assert_eq!(cfg.llm_providers, vec![LlmProvider::Claude]);

        drop(_guard);

        // 4. No file, no env, no CLI → default order.
        let tmp2 = tempfile::tempdir().unwrap();
        let path2 = config_path_for_load(tmp2.path());
        let mut cfg = load_from(&path2);
        cfg.apply_overrides(None);
        assert_eq!(cfg.llm_providers, default_provider_order());
    }

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }
    impl EnvGuard {
        fn set(key: &'static str, val: &str) -> Self {
            let prev = std::env::var(key).ok();
            std::env::set_var(key, val);
            Self { key, prev }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }
}
