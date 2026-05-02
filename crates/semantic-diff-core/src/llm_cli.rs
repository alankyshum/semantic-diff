use crate::config::Config;
use anyhow::Context;
use std::fmt;
use std::future::Future;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// Maximum bytes to read from LLM stdout (1MB). Prevents OOM from malformed CLI output.
const MAX_RESPONSE_BYTES: usize = 1_048_576;
const DEFAULT_PROVIDER_ORDER: &str = "claude,copilot,cursor";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    Claude,
    Copilot,
    Cursor,
}

impl LlmProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Copilot => "copilot",
            Self::Cursor => "cursor",
        }
    }

    /// Stable key used for cost-table lookups (`"<cost_key>:<model>"`).
    /// Currently identical to [`Self::as_str`], but kept as a separate method
    /// so server-side cost lookup and the `default_cost_table` keys share a
    /// single source of truth — changing the wire string here automatically
    /// propagates to both call sites (S1).
    pub fn cost_key(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Copilot => "copilot",
            Self::Cursor => "cursor",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "copilot" => Some(Self::Copilot),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }
}

impl fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmOutputKind {
    Json,
    Text,
}

#[derive(Debug, Clone)]
pub struct LlmInvocation {
    pub provider: LlmProvider,
    pub text: String,
    /// Token usage parsed from provider output, when available (F6/F20).
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone)]
struct CommandRequest {
    display_name: &'static str,
    program: &'static str,
    args: Vec<String>,
    stdin: Option<String>,
    response_parser: ResponseParser,
}

#[derive(Debug, Clone, Copy)]
enum ResponseParser {
    PlainText,
    ClaudeJson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureKind {
    MissingBinary,
    RateLimited,
    Auth,
    Unexpected,
}

#[derive(Debug, Clone)]
struct ProviderFailure {
    provider: LlmProvider,
    kind: FailureKind,
    reason: String,
}

#[derive(Debug, Clone, Default)]
struct ParsedResponse {
    text: String,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cost_usd: Option<f64>,
}

pub fn default_provider_order_csv() -> &'static str {
    DEFAULT_PROVIDER_ORDER
}

pub fn default_provider_order() -> Vec<LlmProvider> {
    parse_provider_order(DEFAULT_PROVIDER_ORDER).expect("default LLM provider order must be valid")
}

pub fn parse_provider_order(raw: &str) -> anyhow::Result<Vec<LlmProvider>> {
    let mut providers = Vec::new();

    for item in raw.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let provider = LlmProvider::parse(trimmed)
            .ok_or_else(|| anyhow::anyhow!("unsupported LLM provider '{trimmed}'"))?;
        if !providers.contains(&provider) {
            providers.push(provider);
        }
    }

    if providers.is_empty() {
        anyhow::bail!("at least one LLM provider must be configured");
    }

    Ok(providers)
}

pub async fn invoke_with_fallback(
    prompt: &str,
    output_kind: LlmOutputKind,
    providers: &[LlmProvider],
    config: &Config,
) -> anyhow::Result<LlmInvocation> {
    invoke_with_fallback_using(providers, |provider| async move {
        invoke_provider(provider, prompt, output_kind, config).await
    })
    .await
}

async fn invoke_with_fallback_using<F, Fut>(
    providers: &[LlmProvider],
    mut invoke: F,
) -> anyhow::Result<LlmInvocation>
where
    F: FnMut(LlmProvider) -> Fut,
    Fut: Future<Output = Result<ParsedResponse, ProviderFailure>>,
{
    let effective_providers = if providers.is_empty() {
        default_provider_order()
    } else {
        providers.to_vec()
    };

    let mut failures = Vec::new();

    for provider in effective_providers.iter().copied() {
        match invoke(provider).await {
            Ok(parsed) => {
                tracing::info!(provider = %provider, "LLM provider selected");
                return Ok(LlmInvocation {
                    provider,
                    text: parsed.text,
                    input_tokens: parsed.input_tokens,
                    output_tokens: parsed.output_tokens,
                    cost_usd: parsed.cost_usd,
                });
            }
            Err(failure) => {
                match failure.kind {
                    FailureKind::MissingBinary => {
                        tracing::debug!(provider = %provider, reason = %failure.reason, "LLM provider unavailable; trying next provider");
                    }
                    FailureKind::RateLimited | FailureKind::Auth => {
                        tracing::warn!(provider = %provider, reason = %failure.reason, "LLM provider failed; falling back");
                    }
                    FailureKind::Unexpected => {
                        tracing::warn!(provider = %provider, reason = %failure.reason, "Unexpected LLM provider failure; falling back");
                    }
                }
                failures.push(failure);
            }
        }
    }

    let tried = effective_providers
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" -> ");
    let reasons = failures
        .iter()
        .map(|failure| format!("{}: {}", failure.provider, failure.reason))
        .collect::<Vec<_>>()
        .join("; ");

    anyhow::bail!("all LLM providers failed after trying {tried}: {reasons}")
}

async fn invoke_provider(
    provider: LlmProvider,
    prompt: &str,
    output_kind: LlmOutputKind,
    config: &Config,
) -> Result<ParsedResponse, ProviderFailure> {
    let requests = requests_for_provider(provider, prompt, output_kind, config);
    let mut missing = Vec::new();

    for request in requests {
        match run_command_request(&request, provider).await {
            Ok(parsed) => return Ok(parsed),
            Err(failure) if failure.kind == FailureKind::MissingBinary => {
                missing.push(request.display_name);
            }
            Err(mut failure) => {
                failure.reason = format!("{} via {}", failure.reason, request.display_name);
                return Err(failure);
            }
        }
    }

    let detail = if missing.is_empty() {
        "no runnable command variants found".to_string()
    } else {
        format!("binary not found on PATH (tried: {})", missing.join(", "))
    };

    Err(ProviderFailure {
        provider,
        kind: FailureKind::MissingBinary,
        reason: detail,
    })
}

fn requests_for_provider(
    provider: LlmProvider,
    prompt: &str,
    output_kind: LlmOutputKind,
    config: &Config,
) -> Vec<CommandRequest> {
    match provider {
        LlmProvider::Claude => {
            let parser = match output_kind {
                LlmOutputKind::Json => ResponseParser::ClaudeJson,
                LlmOutputKind::Text => ResponseParser::PlainText,
            };
            let mut args = vec![
                "-p".to_string(),
                "--output-format".to_string(),
                match output_kind {
                    LlmOutputKind::Json => "json",
                    LlmOutputKind::Text => "text",
                }
                .to_string(),
                "--model".to_string(),
                config.claude_model.clone(),
            ];
            if matches!(output_kind, LlmOutputKind::Json) {
                args.push("--max-turns".to_string());
                args.push("1".to_string());
            }
            vec![CommandRequest {
                display_name: "claude",
                program: "claude",
                args,
                stdin: Some(prompt.to_string()),
                response_parser: parser,
            }]
        }
        LlmProvider::Copilot => {
            let shared_args = vec![
                "-p".to_string(),
                prompt.to_string(),
                "--allow-all-tools".to_string(),
                "--model".to_string(),
                config.copilot_model.clone(),
            ];
            vec![
                CommandRequest {
                    display_name: "copilot",
                    program: "copilot",
                    args: shared_args.clone(),
                    stdin: None,
                    response_parser: ResponseParser::PlainText,
                },
                CommandRequest {
                    display_name: "gh copilot",
                    program: "gh",
                    args: std::iter::once("copilot".to_string())
                        .chain(std::iter::once("--".to_string()))
                        .chain(shared_args)
                        .collect(),
                    stdin: None,
                    response_parser: ResponseParser::PlainText,
                },
            ]
        }
        LlmProvider::Cursor => {
            let shared_args = vec![
                "-p".to_string(),
                "--output-format".to_string(),
                "text".to_string(),
                "--trust".to_string(),
                "--workspace".to_string(),
                ".".to_string(),
                prompt.to_string(),
            ];
            vec![
                CommandRequest {
                    display_name: "cursor-agent",
                    program: "cursor-agent",
                    args: shared_args.clone(),
                    stdin: None,
                    response_parser: ResponseParser::PlainText,
                },
                CommandRequest {
                    display_name: "cursor agent",
                    program: "cursor",
                    args: std::iter::once("agent".to_string())
                        .chain(shared_args)
                        .collect(),
                    stdin: None,
                    response_parser: ResponseParser::PlainText,
                },
            ]
        }
    }
}

async fn run_command_request(
    request: &CommandRequest,
    provider: LlmProvider,
) -> Result<ParsedResponse, ProviderFailure> {
    let mut child = Command::new(request.program)
        .args(&request.args)
        .stdin(if request.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| map_spawn_error(provider, request.display_name, error))?;

    if let Some(stdin_payload) = &request.stdin {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(stdin_payload.as_bytes())
                .await
                .map_err(|error| ProviderFailure {
                    provider,
                    kind: FailureKind::Unexpected,
                    reason: format!("failed writing prompt to {} stdin: {error}", request.display_name),
                })?;
        }
    }

    let stdout_pipe = child.stdout.take().ok_or_else(|| ProviderFailure {
        provider,
        kind: FailureKind::Unexpected,
        reason: format!("failed to capture {} stdout", request.display_name),
    })?;
    let stderr_pipe = child.stderr.take();

    let stdout_fut = async {
        let mut limited = stdout_pipe.take(MAX_RESPONSE_BYTES as u64);
        let mut buf = Vec::with_capacity(8192);
        let bytes_read = limited.read_to_end(&mut buf).await?;
        Ok::<(Vec<u8>, usize), std::io::Error>((buf, bytes_read))
    };
    let stderr_fut = async {
        let mut stderr_buf = Vec::new();
        if let Some(mut stderr) = stderr_pipe {
            stderr.read_to_end(&mut stderr_buf).await.ok();
        }
        stderr_buf
    };

    let (stdout_result, stderr_buf) = tokio::join!(stdout_fut, stderr_fut);
    let (stdout_buf, bytes_read) = stdout_result.map_err(|error| ProviderFailure {
        provider,
        kind: FailureKind::Unexpected,
        reason: format!("failed reading {} stdout: {error}", request.display_name),
    })?;

    if bytes_read >= MAX_RESPONSE_BYTES {
        child.kill().await.ok();
        return Err(ProviderFailure {
            provider,
            kind: FailureKind::Unexpected,
            reason: format!("LLM response exceeded {MAX_RESPONSE_BYTES} byte limit via {}", request.display_name),
        });
    }

    let status = child.wait().await.map_err(|error| ProviderFailure {
        provider,
        kind: FailureKind::Unexpected,
        reason: format!("failed waiting for {}: {error}", request.display_name),
    })?;

    if !status.success() {
        let stderr_text = String::from_utf8_lossy(&stderr_buf);
        let stdout_text = String::from_utf8_lossy(&stdout_buf);
        let diagnostic = summarize_failure_output(&stderr_text, &stdout_text);
        return Err(ProviderFailure {
            provider,
            kind: classify_failure_kind(&diagnostic),
            reason: format!("{} exited with status {status}: {}", request.display_name, diagnostic),
        });
    }

    parse_command_output(provider, request.display_name, request.response_parser, stdout_buf)
}

fn map_spawn_error(
    provider: LlmProvider,
    display_name: &str,
    error: std::io::Error,
) -> ProviderFailure {
    let kind = if error.kind() == std::io::ErrorKind::NotFound {
        FailureKind::MissingBinary
    } else {
        FailureKind::Unexpected
    };
    ProviderFailure {
        provider,
        kind,
        reason: format!("failed to spawn {display_name}: {error}"),
    }
}

fn summarize_failure_output(stderr: &str, stdout: &str) -> String {
    let stderr = compact_message(stderr);
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = compact_message(stdout);
    if !stdout.is_empty() {
        return stdout;
    }

    "no stderr output".to_string()
}

fn compact_message(message: &str) -> String {
    let compact = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let compact = compact.trim();
    if compact.len() > 240 {
        format!("{}…", &compact[..240])
    } else {
        compact.to_string()
    }
}

fn classify_failure_kind(message: &str) -> FailureKind {
    let lower = message.to_ascii_lowercase();

    if [
        "rate limit",
        "rate-limited",
        "429",
        "quota",
        "usage limit",
        "too many requests",
        "throttl",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return FailureKind::RateLimited;
    }

    if [
        "unauthorized",
        "forbidden",
        "not authenticated",
        "not logged",
        "login required",
        "authentication",
        "invalid api key",
        "api key",
        "auth failure",
        "permission denied",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return FailureKind::Auth;
    }

    if ["command not found", "no such file or directory", "not found"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return FailureKind::MissingBinary;
    }

    FailureKind::Unexpected
}

fn parse_command_output(
    provider: LlmProvider,
    display_name: &str,
    response_parser: ResponseParser,
    stdout_buf: Vec<u8>,
) -> Result<ParsedResponse, ProviderFailure> {
    match response_parser {
        ResponseParser::PlainText => String::from_utf8(stdout_buf)
            .map(|text| ParsedResponse { text, ..Default::default() })
            .map_err(|error| ProviderFailure {
                provider,
                kind: FailureKind::Unexpected,
                reason: format!("invalid UTF-8 from {display_name}: {error}"),
            }),
        ResponseParser::ClaudeJson => parse_claude_json_output(provider, display_name, stdout_buf),
    }
}

fn parse_claude_json_output(
    provider: LlmProvider,
    display_name: &str,
    stdout_buf: Vec<u8>,
) -> Result<ParsedResponse, ProviderFailure> {
    let stdout = String::from_utf8(stdout_buf).map_err(|error| ProviderFailure {
        provider,
        kind: FailureKind::Unexpected,
        reason: format!("invalid UTF-8 from {display_name}: {error}"),
    })?;
    let wrapper: serde_json::Value = serde_json::from_str(&stdout).with_context(|| {
        format!("failed to parse {display_name} JSON response")
    }).map_err(|error| ProviderFailure {
        provider,
        kind: FailureKind::Unexpected,
        reason: error.to_string(),
    })?;
    let result_text = wrapper["result"].as_str().ok_or_else(|| ProviderFailure {
        provider,
        kind: FailureKind::Unexpected,
        reason: format!("missing result field in {display_name} JSON output"),
    })?;
    let input_tokens = wrapper["usage"]["input_tokens"].as_u64();
    let output_tokens = wrapper["usage"]["output_tokens"].as_u64();
    let cost_usd = wrapper["total_cost_usd"].as_f64();
    Ok(ParsedResponse {
        text: result_text.to_string(),
        input_tokens,
        output_tokens,
        cost_usd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::ready;

    fn missing(provider: LlmProvider) -> ProviderFailure {
        ProviderFailure {
            provider,
            kind: FailureKind::MissingBinary,
            reason: "binary not found on PATH".to_string(),
        }
    }

    fn unexpected(provider: LlmProvider, reason: &str) -> ProviderFailure {
        ProviderFailure {
            provider,
            kind: FailureKind::Unexpected,
            reason: reason.to_string(),
        }
    }

    #[test]
    fn test_parse_provider_order_dedupes_and_preserves_order() {
        let parsed = parse_provider_order("claude, copilot, cursor,claude").unwrap();
        assert_eq!(parsed, vec![LlmProvider::Claude, LlmProvider::Copilot, LlmProvider::Cursor]);
    }

    #[tokio::test]
    async fn test_invoke_with_fallback_skips_missing_provider() {
        let mut attempted = Vec::new();
        let invocation = invoke_with_fallback_using(
            &[LlmProvider::Claude, LlmProvider::Copilot, LlmProvider::Cursor],
            |provider| {
                attempted.push(provider);
                ready(match provider {
                    LlmProvider::Claude => Err(missing(provider)),
                    LlmProvider::Copilot => Ok(ParsedResponse { text: "copilot response".to_string(), ..Default::default() }),
                    LlmProvider::Cursor => Ok(ParsedResponse { text: "cursor response".to_string(), ..Default::default() }),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(attempted, vec![LlmProvider::Claude, LlmProvider::Copilot]);
        assert_eq!(invocation.provider, LlmProvider::Copilot);
        assert_eq!(invocation.text, "copilot response");
    }

    #[tokio::test]
    async fn test_invoke_with_fallback_reports_all_failures() {
        let error = invoke_with_fallback_using(
            &[LlmProvider::Claude, LlmProvider::Copilot, LlmProvider::Cursor],
            |provider| {
                ready(match provider {
                    LlmProvider::Claude => Err(unexpected(provider, "claude exploded")),
                    LlmProvider::Copilot => Err(missing(provider)),
                    LlmProvider::Cursor => Err(unexpected(provider, "cursor exploded")),
                })
            },
        )
        .await
        .unwrap_err()
        .to_string();

        assert!(error.contains("claude -> copilot -> cursor"));
        assert!(error.contains("claude: claude exploded"));
        assert!(error.contains("copilot: binary not found on PATH"));
        assert!(error.contains("cursor: cursor exploded"));
    }

    #[test]
    fn test_classify_failure_kind_detects_rate_limit() {
        assert_eq!(classify_failure_kind("429 rate limit exceeded"), FailureKind::RateLimited);
    }
}
