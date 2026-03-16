use crate::grouper::llm::LlmBackend;
use crate::theme::ThemeMode;
use serde::Deserialize;
use std::path::PathBuf;

/// User configuration loaded from ~/.config/semantic-diff.json (JSONC supported).
#[derive(Debug, Clone)]
pub struct Config {
    pub preferred_ai_cli: Option<AiCli>,
    pub claude_model: String,
    pub copilot_model: String,
    pub theme_mode: ThemeMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiCli {
    Claude,
    Copilot,
}

/// Raw JSON-serializable config (matches the file format).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawConfig {
    #[serde(rename = "preferred-ai-cli")]
    preferred_ai_cli: Option<AiCli>,
    claude: CliConfig,
    copilot: CliConfig,
    theme: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CliConfig {
    model: Option<String>,
}


/// Model tier for intelligent cross-backend mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ModelTier {
    Fast,     // haiku, gemini-flash
    Balanced, // sonnet, gemini-pro
    Power,    // opus
}

impl Config {
    pub fn default_config() -> Self {
        Self {
            preferred_ai_cli: None,
            claude_model: "haiku".to_string(),
            copilot_model: "gemini-flash".to_string(),
            theme_mode: ThemeMode::Auto,
        }
    }

    /// Resolve the model string to pass to the given backend's CLI.
    pub fn model_for_backend(&self, backend: LlmBackend) -> &str {
        match backend {
            LlmBackend::Claude => &self.claude_model,
            LlmBackend::Copilot => &self.copilot_model,
        }
    }

    /// Detect the best available backend, respecting the user's preference.
    pub fn detect_backend(&self) -> Option<LlmBackend> {
        let claude_ok = which::which("claude").is_ok();
        let copilot_ok = which::which("copilot").is_ok();

        match self.preferred_ai_cli {
            Some(AiCli::Claude) => {
                if claude_ok {
                    Some(LlmBackend::Claude)
                } else if copilot_ok {
                    Some(LlmBackend::Copilot)
                } else {
                    None
                }
            }
            Some(AiCli::Copilot) => {
                if copilot_ok {
                    Some(LlmBackend::Copilot)
                } else if claude_ok {
                    Some(LlmBackend::Claude)
                } else {
                    None
                }
            }
            None => {
                // Default: prefer claude, fallback copilot
                if claude_ok {
                    Some(LlmBackend::Claude)
                } else if copilot_ok {
                    Some(LlmBackend::Copilot)
                } else {
                    None
                }
            }
        }
    }
}

/// Config file path: ~/.config/semantic-diff.json
/// Returns None if home directory cannot be determined (refuses to fall back to cwd).
fn config_path() -> Option<PathBuf> {
    // Explicitly refuse to use cwd as home directory fallback.
    // This prevents a malicious repo from injecting config via .config/semantic-diff.json
    let home = dirs::home_dir()?;
    Some(home.join(".config").join("semantic-diff.json"))
}

/// Default config file content with comments explaining each option.
const DEFAULT_CONFIG: &str = r#"{
  // Which AI CLI to prefer: "claude" or "copilot"
  // Falls back to the other if the preferred one is not installed.
  // If unset, defaults to: claude > copilot
  // "preferred-ai-cli": "claude",

  // Claude CLI settings
  "claude": {
    // Model to use: "haiku" (fast, default), "sonnet" (balanced), "opus" (powerful)
    // Cross-backend models are mapped automatically:
    //   gemini-flash -> haiku, gemini-pro -> sonnet
    "model": "haiku"
  },

  // Copilot CLI settings
  "copilot": {
    // Model to use: "gemini-flash" (fast, default), "sonnet", "opus", "haiku", "gemini-pro"
    "model": "gemini-flash"
  }

  // Theme: "dark", "light", or "auto" (detects from terminal)
  // "theme": "auto"
}
"#;

/// Load config from disk. Creates a default commented config if none exists.
/// Returns default config if home directory cannot be determined.
pub fn load() -> Config {
    let path = match config_path() {
        Some(p) => p,
        None => {
            tracing::warn!("Could not determine home directory, using default config");
            return Config::default_config();
        }
    };

    // Create default config if it doesn't exist
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, DEFAULT_CONFIG);
        tracing::info!("Created default config at {}", path.display());
    }

    // Read and parse
    let content = match std::fs::read_to_string(&path) {
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

    Config {
        preferred_ai_cli: raw.preferred_ai_cli,
        claude_model: resolve_model_for_claude(raw.claude.model.as_deref()),
        copilot_model: resolve_model_for_copilot(raw.copilot.model.as_deref()),
        theme_mode: match raw.theme.as_deref() {
            Some("light") => ThemeMode::Light,
            Some("dark") => ThemeMode::Dark,
            _ => ThemeMode::Auto,
        },
    }
}

/// Map any model name to the closest Claude CLI model.
fn resolve_model_for_claude(model: Option<&str>) -> String {
    let tier = model.map(model_tier).unwrap_or(ModelTier::Fast);
    match tier {
        ModelTier::Fast => "haiku",
        ModelTier::Balanced => "sonnet",
        ModelTier::Power => "opus",
    }
    .to_string()
}

/// Map any model name to the closest Copilot CLI model.
/// Copilot passes model names through directly, but we normalize known aliases.
fn resolve_model_for_copilot(model: Option<&str>) -> String {
    match model {
        Some(m) => {
            let tier = model_tier(m);
            // If it's already a recognized copilot model, pass through
            match m {
                "sonnet" | "opus" | "haiku" | "gemini-flash" | "gemini-pro" => m.to_string(),
                // Otherwise map by tier
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

/// Classify a model name into a performance tier.
fn model_tier(name: &str) -> ModelTier {
    let n = name.to_lowercase();
    if n.contains("flash") || n.contains("haiku") || n == "gpt-4o-mini" || n.ends_with("-mini") {
        ModelTier::Fast
    } else if n.contains("opus") {
        ModelTier::Power
    } else {
        // sonnet, gemini-pro, gpt-4o, etc. → balanced
        ModelTier::Balanced
    }
}

/// Strip // and /* */ comments from JSON text (simple JSONC support).
fn strip_json_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                // Push escaped char as-is
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
                    // Line comment — skip to end of line
                    for rest in chars.by_ref() {
                        if rest == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    // Block comment — skip to */
                    chars.next(); // consume *
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
    fn test_strip_line_comments() {
        let input = r#"{
  // this is a comment
  "key": "value"
}"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["key"], "value");
    }

    #[test]
    fn test_strip_block_comments() {
        let input = r#"{ /* block */ "key": "value" }"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["key"], "value");
    }

    #[test]
    fn test_preserves_strings_with_slashes() {
        let input = r#"{ "url": "https://example.com" }"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["url"], "https://example.com");
    }

    #[test]
    fn test_commented_out_keys_stripped() {
        let input = r#"{
  // "preferred-ai-cli": "claude",
  "claude": { "model": "opus" }
}"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert!(parsed.get("preferred-ai-cli").is_none());
        assert_eq!(parsed["claude"]["model"], "opus");
    }

    #[test]
    fn test_model_tier_mapping() {
        assert_eq!(model_tier("haiku"), ModelTier::Fast);
        assert_eq!(model_tier("gemini-flash"), ModelTier::Fast);
        assert_eq!(model_tier("gpt-4o-mini"), ModelTier::Fast);
        assert_eq!(model_tier("sonnet"), ModelTier::Balanced);
        assert_eq!(model_tier("gemini-pro"), ModelTier::Balanced);
        assert_eq!(model_tier("opus"), ModelTier::Power);
    }

    #[test]
    fn test_resolve_claude_model() {
        assert_eq!(resolve_model_for_claude(Some("gemini-flash")), "haiku");
        assert_eq!(resolve_model_for_claude(Some("sonnet")), "sonnet");
        assert_eq!(resolve_model_for_claude(Some("opus")), "opus");
        assert_eq!(resolve_model_for_claude(Some("gemini-pro")), "sonnet");
        assert_eq!(resolve_model_for_claude(None), "haiku");
    }

    #[test]
    fn test_resolve_copilot_model() {
        assert_eq!(resolve_model_for_copilot(Some("gemini-flash")), "gemini-flash");
        assert_eq!(resolve_model_for_copilot(Some("sonnet")), "sonnet");
        assert_eq!(resolve_model_for_copilot(Some("haiku")), "haiku");
        assert_eq!(resolve_model_for_copilot(None), "gemini-flash");
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
    fn test_config_path_returns_option_not_cwd() {
        // config_path() should return Some with a path under home, never "."
        let path = config_path();
        match path {
            Some(p) => {
                let path_str = p.to_string_lossy();
                assert!(
                    !path_str.starts_with("./"),
                    "config_path should not fall back to cwd, got: {}",
                    path_str
                );
                assert!(
                    path_str.contains(".config/semantic-diff.json"),
                    "config_path should end with .config/semantic-diff.json, got: {}",
                    path_str
                );
            }
            None => {
                // None is acceptable if HOME is not set
            }
        }
    }

    #[test]
    fn test_config_path_no_dot_fallback() {
        // Verify config_path never returns a path starting with "."
        let path = config_path();
        if let Some(p) = path {
            assert_ne!(
                p.components().next().map(|c| c.as_os_str().to_string_lossy().to_string()),
                Some(".".to_string()),
                "config_path must not use '.' as base directory"
            );
        }
    }
}
