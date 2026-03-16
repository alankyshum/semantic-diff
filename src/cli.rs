use clap::Parser;

/// A terminal diff viewer with AI-powered semantic grouping.
///
/// Drop-in replacement for `git diff` — all positional arguments and common
/// flags (--staged, --cached, --merge-base, -- paths) are passed through
/// to `git diff` directly.
///
/// Examples:
///   semantic-diff                        # unstaged changes (same as git diff)
///   semantic-diff HEAD                   # all changes vs HEAD
///   semantic-diff --staged               # staged changes only
///   semantic-diff main..feature          # two-dot range
///   semantic-diff main...feature         # three-dot (merge-base) range
///   semantic-diff HEAD~3 HEAD -- src/    # specific commits + path filter
#[derive(Parser, Debug)]
#[command(name = "semantic-diff", version, about)]
pub struct Cli {
    /// Arguments passed through to `git diff` (commits, ranges, --staged, -- paths, etc.)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub git_args: Vec<String>,
}

impl Cli {
    /// Build the full argument list for `git diff`, prepending `-M` for rename detection.
    pub fn git_diff_args(&self) -> Vec<String> {
        let mut args = vec!["diff".to_string(), "-M".to_string()];
        args.extend(self.git_args.iter().cloned());
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_args_produces_bare_diff() {
        let cli = Cli { git_args: vec![] };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M"]);
    }

    #[test]
    fn test_head_arg() {
        let cli = Cli {
            git_args: vec!["HEAD".to_string()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "HEAD"]);
    }

    #[test]
    fn test_staged_flag() {
        let cli = Cli {
            git_args: vec!["--staged".to_string()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "--staged"]);
    }

    #[test]
    fn test_two_dot_range() {
        let cli = Cli {
            git_args: vec!["main..feature".to_string()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "main..feature"]);
    }

    #[test]
    fn test_three_dot_range() {
        let cli = Cli {
            git_args: vec!["main...feature".to_string()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "main...feature"]);
    }

    #[test]
    fn test_two_refs() {
        let cli = Cli {
            git_args: vec!["main".to_string(), "feature".to_string()],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "main", "feature"]
        );
    }

    #[test]
    fn test_path_limiter() {
        let cli = Cli {
            git_args: vec![
                "HEAD".to_string(),
                "--".to_string(),
                "src/".to_string(),
            ],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "HEAD", "--", "src/"]
        );
    }

    #[test]
    fn test_cached_alias() {
        let cli = Cli {
            git_args: vec!["--cached".to_string()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "--cached"]);
    }

    // --- Stress / edge-case tests ---

    #[test]
    fn test_head_tilde_syntax() {
        let cli = Cli {
            git_args: vec!["HEAD~3".to_string()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "HEAD~3"]);
    }

    #[test]
    fn test_head_caret_syntax() {
        let cli = Cli {
            git_args: vec!["HEAD^".to_string()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "HEAD^"]);
    }

    #[test]
    fn test_sha_refs() {
        let cli = Cli {
            git_args: vec![
                "abc1234".to_string(),
                "def5678".to_string(),
            ],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "abc1234", "def5678"]
        );
    }

    #[test]
    fn test_full_sha() {
        let sha = "a".repeat(40);
        let cli = Cli {
            git_args: vec![sha.clone()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", &sha]);
    }

    #[test]
    fn test_staged_with_ref() {
        let cli = Cli {
            git_args: vec!["--staged".to_string(), "HEAD~1".to_string()],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "--staged", "HEAD~1"]
        );
    }

    #[test]
    fn test_multiple_path_limiters() {
        let cli = Cli {
            git_args: vec![
                "HEAD".to_string(),
                "--".to_string(),
                "src/".to_string(),
                "tests/".to_string(),
                "Cargo.toml".to_string(),
            ],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "HEAD", "--", "src/", "tests/", "Cargo.toml"]
        );
    }

    #[test]
    fn test_two_dot_range_with_paths() {
        let cli = Cli {
            git_args: vec![
                "main..feature".to_string(),
                "--".to_string(),
                "src/".to_string(),
            ],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "main..feature", "--", "src/"]
        );
    }

    #[test]
    fn test_three_dot_range_with_paths() {
        let cli = Cli {
            git_args: vec![
                "origin/main...HEAD".to_string(),
                "--".to_string(),
                "*.rs".to_string(),
            ],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "origin/main...HEAD", "--", "*.rs"]
        );
    }

    #[test]
    fn test_merge_base_flag() {
        let cli = Cli {
            git_args: vec!["--merge-base".to_string(), "main".to_string()],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "--merge-base", "main"]
        );
    }

    #[test]
    fn test_no_index_flag() {
        let cli = Cli {
            git_args: vec![
                "--no-index".to_string(),
                "file_a.txt".to_string(),
                "file_b.txt".to_string(),
            ],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "--no-index", "file_a.txt", "file_b.txt"]
        );
    }

    #[test]
    fn test_many_positional_args_stress() {
        let args: Vec<String> = (0..100).map(|i| format!("path_{i}.rs")).collect();
        let cli = Cli {
            git_args: args.clone(),
        };
        let result = cli.git_diff_args();
        assert_eq!(result.len(), 102); // "diff" + "-M" + 100 paths
        assert_eq!(result[0], "diff");
        assert_eq!(result[1], "-M");
        assert_eq!(result[2], "path_0.rs");
        assert_eq!(result[101], "path_99.rs");
    }

    #[test]
    fn test_unicode_path() {
        let cli = Cli {
            git_args: vec![
                "HEAD".to_string(),
                "--".to_string(),
                "src/日本語/ファイル.rs".to_string(),
            ],
        };
        let result = cli.git_diff_args();
        assert_eq!(result[4], "src/日本語/ファイル.rs");
    }

    #[test]
    fn test_path_with_spaces() {
        let cli = Cli {
            git_args: vec![
                "--".to_string(),
                "path with spaces/file.rs".to_string(),
            ],
        };
        let result = cli.git_diff_args();
        assert_eq!(result[3], "path with spaces/file.rs");
    }

    #[test]
    fn test_at_upstream_syntax() {
        let cli = Cli {
            git_args: vec!["@{upstream}".to_string()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "@{upstream}"]);
    }

    #[test]
    fn test_stash_ref() {
        let cli = Cli {
            git_args: vec!["stash@{0}".to_string()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "stash@{0}"]);
    }

    #[test]
    fn test_remote_tracking_branch() {
        let cli = Cli {
            git_args: vec![
                "origin/main".to_string(),
                "origin/feature/my-branch".to_string(),
            ],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "origin/main", "origin/feature/my-branch"]
        );
    }

    #[test]
    fn test_tag_ref() {
        let cli = Cli {
            git_args: vec!["v1.0.0".to_string(), "v2.0.0".to_string()],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "v1.0.0", "v2.0.0"]
        );
    }

    #[test]
    fn test_diff_filter_flag_passthrough() {
        let cli = Cli {
            git_args: vec!["--diff-filter=ACMR".to_string(), "HEAD".to_string()],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "--diff-filter=ACMR", "HEAD"]
        );
    }

    #[test]
    fn test_stat_flag_passthrough() {
        let cli = Cli {
            git_args: vec!["--stat".to_string(), "HEAD".to_string()],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "--stat", "HEAD"]
        );
    }

    #[test]
    fn test_name_only_flag_passthrough() {
        let cli = Cli {
            git_args: vec!["--name-only".to_string()],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "--name-only"]
        );
    }

    #[test]
    fn test_combined_flags_and_ranges() {
        let cli = Cli {
            git_args: vec![
                "--staged".to_string(),
                "--diff-filter=M".to_string(),
                "HEAD~5".to_string(),
                "--".to_string(),
                "src/".to_string(),
            ],
        };
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "--staged", "--diff-filter=M", "HEAD~5", "--", "src/"]
        );
    }

    #[test]
    fn test_empty_string_arg() {
        let cli = Cli {
            git_args: vec!["".to_string()],
        };
        let result = cli.git_diff_args();
        assert_eq!(result, vec!["diff", "-M", ""]);
    }

    #[test]
    fn test_double_dash_only() {
        let cli = Cli {
            git_args: vec!["--".to_string()],
        };
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "--"]);
    }

    #[test]
    fn test_clap_parse_no_args() {
        // Simulate: semantic-diff (no arguments)
        let cli = Cli::try_parse_from(["semantic-diff"]).unwrap();
        assert!(cli.git_args.is_empty());
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M"]);
    }

    #[test]
    fn test_clap_parse_head() {
        let cli = Cli::try_parse_from(["semantic-diff", "HEAD"]).unwrap();
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "HEAD"]);
    }

    #[test]
    fn test_clap_parse_staged() {
        let cli = Cli::try_parse_from(["semantic-diff", "--staged"]).unwrap();
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "--staged"]);
    }

    #[test]
    fn test_clap_parse_cached() {
        let cli = Cli::try_parse_from(["semantic-diff", "--cached"]).unwrap();
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "--cached"]);
    }

    #[test]
    fn test_clap_parse_two_dot_range() {
        let cli = Cli::try_parse_from(["semantic-diff", "main..feature"]).unwrap();
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "main..feature"]);
    }

    #[test]
    fn test_clap_parse_three_dot_range() {
        let cli = Cli::try_parse_from(["semantic-diff", "main...feature"]).unwrap();
        assert_eq!(cli.git_diff_args(), vec!["diff", "-M", "main...feature"]);
    }

    #[test]
    fn test_clap_parse_two_refs() {
        let cli = Cli::try_parse_from(["semantic-diff", "abc123", "def456"]).unwrap();
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "abc123", "def456"]
        );
    }

    #[test]
    fn test_clap_parse_ref_with_paths() {
        let cli = Cli::try_parse_from([
            "semantic-diff",
            "HEAD~3",
            "--",
            "src/main.rs",
            "src/lib.rs",
        ])
        .unwrap();
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "HEAD~3", "--", "src/main.rs", "src/lib.rs"]
        );
    }

    #[test]
    fn test_clap_parse_complex_scenario() {
        let cli = Cli::try_parse_from([
            "semantic-diff",
            "--staged",
            "--diff-filter=ACMR",
            "HEAD~5",
            "--",
            "src/",
            "tests/",
        ])
        .unwrap();
        assert_eq!(
            cli.git_diff_args(),
            vec![
                "diff",
                "-M",
                "--staged",
                "--diff-filter=ACMR",
                "HEAD~5",
                "--",
                "src/",
                "tests/"
            ]
        );
    }

    #[test]
    fn test_clap_parse_merge_base() {
        let cli =
            Cli::try_parse_from(["semantic-diff", "--merge-base", "main"]).unwrap();
        assert_eq!(
            cli.git_diff_args(),
            vec!["diff", "-M", "--merge-base", "main"]
        );
    }

    #[test]
    fn test_clap_version_does_not_conflict() {
        // --version is handled by clap, should not be passed through
        let result = Cli::try_parse_from(["semantic-diff", "--version"]);
        // clap exits with a DisplayVersion error for --version
        assert!(result.is_err());
    }

    #[test]
    fn test_clap_help_does_not_conflict() {
        let result = Cli::try_parse_from(["semantic-diff", "--help"]);
        assert!(result.is_err());
    }
}
