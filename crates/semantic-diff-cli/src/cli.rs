use clap::Parser;

/// A web-based diff viewer with AI-powered semantic grouping.
///
/// Examples:
///   semantic-diff                          # unstaged changes
///   semantic-diff HEAD~3..HEAD             # commit range
///   semantic-diff --staged                 # staged changes
///   semantic-diff --diff patch.patch       # from a diff file
///   semantic-diff --pr owner/repo#123      # from a PR
///   git diff HEAD~5 | semantic-diff --stdin
#[derive(Parser, Debug)]
#[command(name = "semantic-diff", version, about)]
pub struct Cli {
    // ---- Input sources (mutually exclusive; first match wins) ----

    /// Read unified diff from a file instead of running git diff.
    #[arg(long, value_name = "FILE", conflicts_with_all = &["stdin", "pr"])]
    pub diff: Option<std::path::PathBuf>,

    /// Read unified diff from stdin (auto-detected if stdin is piped).
    #[arg(long, conflicts_with_all = &["diff", "pr"])]
    pub stdin: bool,

    /// Fetch diff from a GitHub PR via `gh pr diff`.
    /// Accepts: https://github.com/owner/repo/pull/N or owner/repo#N
    #[arg(long, value_name = "URL_OR_REF", conflicts_with_all = &["diff", "stdin"])]
    pub pr: Option<String>,

    /// Arguments passed through to `git diff -M <args>` (default: unstaged changes).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub git_args: Vec<String>,

    // ---- Output / serving ----

    /// Port to serve on. Defaults to OS-assigned (0). Env: SEMANTIC_DIFF_PORT.
    #[arg(long, value_name = "PORT", default_value = "0", env = "SEMANTIC_DIFF_PORT")]
    pub port: u16,

    /// Directory to write result.json to.
    /// Defaults to ~/.local/share/semantic-diff/results/<id>/
    #[arg(long, value_name = "DIR")]
    pub output: Option<std::path::PathBuf>,

    /// Don't auto-open browser after starting the server.
    #[arg(long)]
    pub no_open: bool,

    /// Skip LLM grouping/review; just render the raw diff in the SPA.
    #[arg(long)]
    pub no_llm: bool,

    /// Comma-separated LLM CLI fallback order. Env: SEMANTIC_DIFF_LLM_PROVIDERS.
    #[arg(long, value_name = "LIST", default_value = "claude,copilot,cursor", env = "SEMANTIC_DIFF_LLM_PROVIDERS")]
    pub llm_providers: String,

    /// Skip review, serve an existing result.json (replay mode).
    #[arg(long, value_name = "FILE")]
    pub result: Option<std::path::PathBuf>,

    /// Human label shown in the SPA header.
    #[arg(long, value_name = "STR")]
    pub title: Option<String>,
}

impl Cli {
    /// Parse argv with smart partitioning: semantic-diff's own flags can appear
    /// in any order alongside git args. We pre-partition argv so that git args
    /// (positional revisions and unknown flags like `--staged`, `--cached`)
    /// are routed to `git_args`, while our known flags are consumed by clap.
    ///
    /// Without this, clap's `trailing_var_arg` treats the first unknown arg as
    /// the start of trailing positionals, sweeping all subsequent flags
    /// (including `--no-llm`, `--title`, etc.) into `git_args` and forwarding
    /// them to `git diff`, which then fails.
    pub fn parse_smart() -> Self {
        let argv: Vec<String> = std::env::args().collect();
        let (own, git) = partition_argv(&argv[1..]);
        let mut rebuilt: Vec<String> = std::iter::once(argv[0].clone()).chain(own).collect();
        if !git.is_empty() {
            rebuilt.push("--".to_string());
            rebuilt.extend(git);
        }
        <Self as Parser>::parse_from(rebuilt)
    }

    /// Build the full argument list for `git diff`, prepending `-M` for rename detection.
    pub fn git_diff_args(&self) -> Vec<String> {
        let mut args = vec!["diff".to_string(), "-M".to_string()];
        args.extend(self.git_args.iter().cloned());
        args
    }

    /// Determine if stdin should be used as diff source.
    pub fn use_stdin(&self) -> bool {
        use std::io::IsTerminal;
        self.stdin
            || (!std::io::stdin().is_terminal()
                && self.diff.is_none()
                && self.pr.is_none()
                && self.git_args.is_empty())
    }
}

// INVARIANT: VALUE_FLAGS and BOOL_FLAGS must enumerate every long flag on the
// `Cli` struct above. The `partition_argv` function uses these lists to route
// tokens between this CLI and the inner `git diff` invocation; any flag that is
// missing here will be silently forwarded to `git diff` and almost certainly
// cause "fatal: ambiguous argument" errors. The `allowlists_match_cli_definition`
// test below enforces this at `cargo test` time via clap reflection.

/// Flags this CLI owns that take a value (consume the next argv token).
const VALUE_FLAGS: &[&str] = &[
    "--diff", "--pr", "--port", "--output", "--llm-providers", "--result", "--title",
];

/// Boolean flags this CLI owns.
const BOOL_FLAGS: &[&str] = &[
    "--stdin", "--no-open", "--no-llm", "--help", "-h", "--version", "-V",
];

/// Split argv (without the program name) into (own_args, git_args).
///
/// Walks tokens left to right. Known flags are routed to `own_args`; unknown
/// tokens (positional revisions, unrecognized flags like `--staged`) go to
/// `git_args`. Once `--` is seen, everything after it goes verbatim to
/// `git_args`. We do NOT terminate own-arg scanning at the first unknown
/// token — we keep scanning so users can interleave (`semantic-diff --staged
/// --no-llm`).
fn partition_argv(args: &[String]) -> (Vec<String>, Vec<String>) {
    let mut own = Vec::new();
    let mut git = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            // Pass remainder as git args verbatim (drop the `--` itself).
            git.extend(args[i + 1..].iter().cloned());
            break;
        }
        // Match `--flag=value` form
        let bare = a.split_once('=').map(|(k, _)| k).unwrap_or(a.as_str());
        if VALUE_FLAGS.contains(&bare) {
            own.push(a.clone());
            // If form is `--flag value` (no `=`), consume the next token too.
            if !a.contains('=') && i + 1 < args.len() {
                own.push(args[i + 1].clone());
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if BOOL_FLAGS.contains(&bare) {
            own.push(a.clone());
            i += 1;
            continue;
        }
        // Unknown — route to git.
        git.push(a.clone());
        i += 1;
    }
    (own, git)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_args_produces_bare_diff() {
        let cli = Cli::try_parse_from(["semantic-diff"]).unwrap();
        assert!(cli.git_args.is_empty());
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M"]);
    }

    #[test]
    fn test_head_arg() {
        let cli = Cli::try_parse_from(["semantic-diff", "HEAD"]).unwrap();
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "HEAD"]);
    }

    #[test]
    fn test_staged_flag() {
        let cli = Cli::try_parse_from(["semantic-diff", "--staged"]).unwrap();
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "--staged"]);
    }

    #[test]
    fn test_two_dot_range() {
        let cli = Cli::try_parse_from(["semantic-diff", "main..feature"]).unwrap();
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "main..feature"]);
    }

    #[test]
    fn test_diff_file_flag() {
        let cli = Cli::try_parse_from(["semantic-diff", "--diff", "patch.patch"]).unwrap();
        assert!(cli.diff.is_some());
        assert!(cli.git_args.is_empty());
    }

    #[test]
    fn test_no_llm_flag() {
        let cli = Cli::try_parse_from(["semantic-diff", "--no-llm"]).unwrap();
        assert!(cli.no_llm);
    }

    #[test]
    fn test_pr_flag() {
        let cli = Cli::try_parse_from(["semantic-diff", "--pr", "owner/repo#5"]).unwrap();
        assert_eq!(cli.pr.as_deref(), Some("owner/repo#5"));
    }

    #[test]
    fn test_port_default() {
        let cli = Cli::try_parse_from(["semantic-diff"]).unwrap();
        assert_eq!(cli.port, 0);
    }

    #[test]
    fn test_port_custom() {
        let cli = Cli::try_parse_from(["semantic-diff", "--port", "8765"]).unwrap();
        assert_eq!(cli.port, 8765);
    }

    #[test]
    fn test_title_flag() {
        let cli = Cli::try_parse_from(["semantic-diff", "--title", "My PR"]).unwrap();
        assert_eq!(cli.title.as_deref(), Some("My PR"));
    }

    #[test]
    fn test_llm_providers_flag() {
        let cli = Cli::try_parse_from(["semantic-diff", "--llm-providers", "cursor,copilot"]).unwrap();
        assert_eq!(cli.llm_providers, "cursor,copilot");
    }

    #[test]
    fn test_partition_staged_with_own_flags_after() {
        // Regression: `--staged --no-llm --no-open --title test` was previously
        // forwarded entirely to git diff because clap's trailing_var_arg swept
        // all subsequent flags into git_args.
        let argv: Vec<String> = ["--staged", "--no-llm", "--no-open", "--title", "test"]
            .iter().map(|s| s.to_string()).collect();
        let (own, git) = partition_argv(&argv);
        assert_eq!(own, vec!["--no-llm", "--no-open", "--title", "test"]);
        assert_eq!(git, vec!["--staged"]);
    }

    #[test]
    fn test_partition_positional_revision() {
        let argv: Vec<String> = ["HEAD~1..HEAD", "--no-llm"].iter().map(|s| s.to_string()).collect();
        let (own, git) = partition_argv(&argv);
        assert_eq!(own, vec!["--no-llm"]);
        assert_eq!(git, vec!["HEAD~1..HEAD"]);
    }

    #[test]
    fn test_partition_double_dash_separator() {
        let argv: Vec<String> = ["--no-llm", "--", "--staged", "path/to/file"]
            .iter().map(|s| s.to_string()).collect();
        let (own, git) = partition_argv(&argv);
        assert_eq!(own, vec!["--no-llm"]);
        assert_eq!(git, vec!["--staged", "path/to/file"]);
    }

    #[test]
    fn test_partition_value_flag_with_equals() {
        let argv: Vec<String> = ["--title=My PR", "HEAD"].iter().map(|s| s.to_string()).collect();
        let (own, git) = partition_argv(&argv);
        assert_eq!(own, vec!["--title=My PR"]);
        assert_eq!(git, vec!["HEAD"]);
    }

    #[test]
    fn allowlists_match_cli_definition() {
        use clap::{ArgAction, CommandFactory};
        let cmd = Cli::command();
        for arg in cmd.get_arguments() {
            // Skip positional args (no long form).
            let Some(long) = arg.get_long() else { continue };
            let key = format!("--{}", long);
            let takes_value = matches!(
                arg.get_action(),
                ArgAction::Set | ArgAction::Append
            );
            if takes_value {
                assert!(
                    VALUE_FLAGS.contains(&key.as_str()),
                    "VALUE_FLAGS is missing `{}` (declared on Cli but not in the allowlist). \
                     Add it to VALUE_FLAGS in cli.rs.",
                    key
                );
            } else {
                assert!(
                    BOOL_FLAGS.contains(&key.as_str()),
                    "BOOL_FLAGS is missing `{}` (declared on Cli but not in the allowlist). \
                     Add it to BOOL_FLAGS in cli.rs.",
                    key
                );
            }
        }
    }

    #[test]
    fn test_partition_trailing_value_flag_no_value() {
        // Regression: argv ending in a value-taking flag with no value should
        // route the bare flag to `own` (and let clap produce its own error).
        let argv: Vec<String> = ["--title"].iter().map(|s| s.to_string()).collect();
        let (own, git) = partition_argv(&argv);
        assert_eq!(own, vec!["--title"]);
        assert!(git.is_empty());
    }
}
