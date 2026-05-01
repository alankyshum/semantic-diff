use serde::Deserialize;
use std::path::PathBuf;

/// User configuration loaded from ~/.config/semantic-diff.json (JSONC supported).
#[derive(Debug, Clone)]
pub struct Config {
    pub preferred_ai_cli: Option<AiCli>,
    pub claude_model: String,
    pub copilot_model: String,
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
        }
    }

}

/// Config file path: ~/.config/semantic-diff.json
fn config_path() -> Option<PathBuf> {
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
    "model": "haiku"
  },

  // Copilot CLI settings
  "copilot": {
    // Model to use: "gemini-flash" (fast, default), "sonnet", "opus", "haiku", "gemini-pro"
    "model": "gemini-flash"
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

    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, DEFAULT_CONFIG);
        tracing::info!("Created default config at {}", path.display());
    }

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
}
