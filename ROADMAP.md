# semantic-diff Product Roadmap

## Executive Summary

**The bottleneck in software engineering has shifted from writing code to understanding what changed.** AI coding agents (Claude Code, Cursor, Copilot) now generate code faster than developers can review it. 96% of developers report not fully trusting AI-generated code (Jan 2026 survey), yet AI-assisted repositories see 39% higher cognitive complexity and 47% more PRs per author carrying 1.7x more issues.

semantic-diff sits at the intersection of three converging mega-trends:
1. **The review debt crisis** — developers spend as much time reviewing AI code as writing it manually
2. **The terminal-first renaissance** — Ghostty, Neovim+tmux, Claude Code driving developers back to CLI
3. **The semantic gap** — existing diff tools show *what* changed but not *why*

No other tool combines **semantic grouping by intent** + **terminal-native TUI** + **AI-powered analysis** at **review time**. This is a category-defining opportunity.

---

## Market Context

### The Problem (validated by community sentiment)

> *"AI tools find critical bugs... but the signal-to-noise ratio is poor. It's hard to get it not to tell you 20 highly speculative reasons why the code is problematic along with the one critical error."* — Hacker News, Jan 2026

> *"The AI doesn't know your business logic, your performance constraints, or your security model."* — Developer blog

> *"Developers feel 20% faster using AI tools, but actual task completion often slows down by 19% due to hidden overhead of debugging and reviewing machine-generated code."* — Research data, 2026

Key pain points developers report:
- **Illusion of correctness**: AI code looks professional but masks logical/security flaws
- **Loss of ownership**: Developers no longer have mental models of code they didn't write
- **Massive diff dumps**: AI agents generate 100 lines for a 10-line task, inflating review surface area
- **No intent visibility**: Traditional diffs show line changes, not the *purpose* behind them
- **Cross-file blindness**: Related changes spread across files are invisible in flat file-by-file diffs
- **45% of AI-generated code** contains OWASP Top 10 vulnerabilities (Veracode 2025/2026)

### Market Size

- Global AI coding assistant market: **$8.5B by 2026** (45% CAGR)
- Claude Code alone: **$2.5B revenue run-rate**, weekly active users doubled in first 6 weeks of 2026
- The "review tooling" segment is nascent — most investment goes into *generation*, not *review*

### Competitive Landscape

| Category | Tools | What they do | Gap semantic-diff fills |
|----------|-------|-------------|------------------------|
| **PR summarization bots** | CodeRabbit, Ellipsis, Sourcery, Qodo | LLM-generated PR summaries | Web-only, no terminal TUI, no local review, no semantic grouping of the diff itself |
| **Structural diff** | Difftastic (tree-sitter) | AST-aware diffing, ignores formatting | No AI intent grouping, no TUI chrome, read-only viewing |
| **Pretty diff** | Delta, diff-so-fancy | Syntax highlighting for diffs | Purely cosmetic, no semantic understanding |
| **Semantic commits** | VibeGit | Groups changes into atomic commits | Commit-time only, not review-time; no TUI |
| **Stacked PRs** | Graphite | Forces small changesets | Workflow tool, not a diff viewer |
| **Web review** | Gnosis, LightLayer | AI-powered code review | Not terminal-native (Electron/web) |
| **Git TUI** | lazygit, gitui, tig | Terminal git interfaces | No AI, no semantic grouping |

**semantic-diff's unique position**: The only tool that does AI-powered semantic grouping in a terminal-native TUI at review time.

---

## User Personas

### P1: The Claude Code Power User (Primary)
- **Who**: Senior dev using Ghostty + tmux + Claude Code daily
- **Workflow**: Prompts agent, watches files change, needs to review before committing
- **Pain**: `git diff` shows 200+ lines across 15 files with no structure; switches to IDE to make sense of it
- **Value prop**: Review without leaving the terminal; see changes grouped by intent

### P2: The Tech Lead / Reviewer
- **Who**: Reviews PRs from team members (increasingly AI-assisted)
- **Workflow**: Gets large PRs, needs to understand architectural impact quickly
- **Pain**: GitHub PR UI shows file-by-file; can't see cross-cutting concerns; wastes 30min on what should be 5min
- **Value prop**: Semantic groups surface "auth refactor" vs "test coverage" vs "config changes" instantly

### P3: The Security-Conscious Engineer
- **Who**: Senior/staff eng who audits AI-generated code for vulnerabilities
- **Workflow**: Needs to identify high-risk changes (auth, input handling, crypto) in large diffs
- **Pain**: AI code "looks right" but hides subtle vulnerabilities; no risk prioritization in diff tools
- **Value prop**: AI-powered risk grouping highlights security-critical changes first

### P4: The Open Source Maintainer
- **Who**: Receives AI-generated contributions from external contributors
- **Workflow**: Reviews unfamiliar code from strangers, needs to understand intent quickly
- **Pain**: AI-generated PRs are large, lack context, and require extensive back-and-forth
- **Value prop**: Instant semantic summary of what a contribution does and why

---

## Product Roadmap

### Phase 1: Core Hardening (v0.8 - v0.9) — "Make it rock-solid"
**Goal**: Make the existing experience production-quality for daily use.

| Feature | Priority | Rationale |
|---------|----------|-----------|
| **Staged diff support** (`git diff --cached`) | P0 | Users need to review what they're about to commit, not just unstaged changes |
| **Branch diff support** (`main..HEAD`) | P0 | Review all changes on a feature branch — the primary PR review use case |
| **Commit range diffs** (`HEAD~3..HEAD`) | P1 | Review recent commits, essential for "what did the agent just do?" |
| **Pipe input support** (`git diff \| semantic-diff`) | P1 | Unix philosophy; composability with any diff source |
| **Performance: large diffs** | P0 | AI agents generate large changes; TUI must stay responsive at 1000+ lines |
| **Error recovery** | P1 | Graceful handling when AI backend is slow/unavailable; better loading states |
| **Cross-platform testing** | P1 | Linux support (many Claude Code users are on remote Linux boxes via SSH) |

### Phase 2: Intelligence Layer (v1.0 - v1.2) — "Understand the diff"
**Goal**: Make semantic-diff the smartest way to review changes.

| Feature | Priority | Rationale |
|---------|----------|-----------|
| **Risk-scored grouping** | P0 | Groups tagged with risk level (high/medium/low); auth changes, dependency updates flagged automatically |
| **Suggested review order** | P0 | AI suggests which groups to review first based on risk and dependency; "read the API contract first, then implementation, then tests" |
| **Change impact summary** | P1 | One-paragraph natural language summary per group: "This group refactors the auth middleware to use JWT tokens instead of session cookies" |
| **Test coverage awareness** | P2 | Highlight changed code that lacks test coverage; flag untested new code paths |
| **Dependency graph awareness** | P2 | Show which other files/functions are affected by a change even if not in the diff |
| **Inline AI annotations** | P1 | Hover/expand on a hunk to get AI explanation of what it does and potential issues |

### Phase 3: Integration & Workflow (v1.3 - v1.5) — "Fit into every workflow"
**Goal**: Make semantic-diff the default review step in every developer's workflow.

| Feature | Priority | Rationale |
|---------|----------|-----------|
| **Claude Code hook integration** | P0 | Auto-trigger semantic-diff after Claude Code makes changes (PostToolUse hook) |
| **`--json` output mode** | P0 | Machine-readable output for CI/CD pipelines, other tools, scripting |
| **GitHub PR integration** | P1 | `semantic-diff --pr 123` fetches PR diff from GitHub and displays semantically grouped |
| **Git commit integration** | P1 | `semantic-diff --commit` shows grouped diff then prompts for commit; atomic commits per group |
| **MCP server mode** | P1 | Expose semantic-diff as an MCP tool so Claude Code can query semantic groupings programmatically |
| **Export to markdown** | P2 | Generate review summary as markdown for PR descriptions or Slack |
| **GitLab/Bitbucket support** | P2 | Extend PR integration beyond GitHub |

### Phase 4: Advanced Intelligence (v2.0+) — "The AI review co-pilot"
**Goal**: Transform from a viewer into an active review assistant.

| Feature | Priority | Rationale |
|---------|----------|-----------|
| **Interactive review mode** | P0 | Mark groups as "reviewed/approved/needs-work"; track review progress |
| **AI-generated review comments** | P1 | AI suggests specific concerns per group; developer approves/dismisses before posting to PR |
| **Security vulnerability detection** | P1 | Flag OWASP Top 10 patterns in changed code; 45% of AI code has vulns — massive opportunity |
| **Before/after behavior comparison** | P2 | AI explains behavioral differences: "This function now returns null instead of throwing on invalid input" |
| **Cross-PR semantic history** | P2 | Track how semantic groups evolve across commits: "The auth refactor started in PR #42 and continued here" |
| **Team review dashboard** | P3 | For tech leads: see which PRs need review, grouped by semantic theme across the team |
| **Custom grouping rules** | P2 | User-defined rules: "Always group migration files separately" or "Flag any changes to /auth/*" |

### Phase 5: Distribution & Growth (Ongoing)
**Goal**: Reach critical mass in the terminal-first developer community.

| Initiative | Priority | Rationale |
|------------|----------|-----------|
| **Homebrew core** | P0 | Move from tap to core formula; 10x discovery |
| **Static binary releases** | P0 | `curl \| sh` installer; don't force Rust toolchain on users |
| **Nix package** | P1 | High overlap with terminal-first power users |
| **AUR package** | P1 | Linux terminal users |
| **HN launch post** | P0 | Benchmark-heavy technical blog post; the proven Rust CLI launch playbook |
| **Claude Code docs listing** | P1 | Get listed as recommended review tool in Claude Code documentation |
| **YouTube demo** | P1 | 2-min "before/after" showing semantic-diff vs raw `git diff` |
| **Integration with popular dotfiles** | P2 | Include in curated "terminal-first AI dev setup" posts |

---

## Strategic Bets & Hypotheses

### Hypothesis 1: "Review is the new bottleneck"
**Bet**: As AI coding agents become ubiquitous, review tooling will become as important as the agents themselves. The market for review tools will grow proportionally to AI code generation adoption.

**Validation signals**: Growing HN/Reddit discussion of review pain; enterprise policies mandating AI code review; EU AI Act compliance requirements (Aug 2026).

### Hypothesis 2: "Terminal-native wins for power users"
**Bet**: The highest-value developers (senior/staff engineers, tech leads) are disproportionately terminal-first. They will prefer a terminal-native review tool over yet another web UI.

**Validation signals**: Ghostty adoption, Claude Code growth ($2.5B run rate), Neovim renaissance, lazygit popularity (40k+ stars).

### Hypothesis 3: "Semantic grouping is the key differentiator"
**Bet**: Showing changes grouped by intent (not file path) is a 10x improvement in review comprehension. This is the core innovation that justifies a new tool.

**Validation signals**: User feedback after trying semantic-diff; time-to-understand benchmarks vs raw diff.

### Hypothesis 4: "The Claude Code ecosystem is the distribution wedge"
**Bet**: Claude Code's hook system and MCP protocol create a natural integration point. Being the "default review tool for Claude Code users" is the fastest path to adoption.

**Validation signals**: Claude Code's explosive growth; hook ecosystem forming; first-mover advantage in this niche.

---

## Anti-Roadmap (What We Won't Build)

- **IDE plugin**: We are terminal-native. IDE users have existing review tools. Don't dilute focus.
- **PR bot / GitHub App**: CodeRabbit, Ellipsis already own this. We solve a different problem (local review, not automated commenting).
- **Code generation**: We review code, we don't write it. Stay in the review lane.
- **Web UI**: Electron/web is a different product. Terminal-native is the moat.
- **AI model hosting**: We use external CLIs (Claude, Copilot). Don't build inference infrastructure.

---

## Success Metrics

| Metric | Target (6 months) | Target (12 months) |
|--------|-------------------|---------------------|
| GitHub stars | 1,000 | 5,000 |
| crates.io downloads | 5,000 | 25,000 |
| Weekly active users (est.) | 500 | 3,000 |
| HN front page posts | 1 | 3 |
| Average review time reduction | 30% (self-reported) | 50% (measured) |

---

## Immediate Next Steps (This Quarter)

1. **Ship v0.8** with staged diff + branch diff support (unblocks the primary use case)
2. **Write a benchmark blog post**: "Reviewing a 500-line AI-generated diff: semantic-diff vs git diff vs GitHub UI" — with timing data
3. **Submit to Homebrew core**
4. **Create GitHub Releases with static binaries** (Linux amd64/arm64, macOS amd64/arm64)
5. **Post to HN**: "Show HN: semantic-diff — AI-powered terminal diff viewer that groups changes by intent"
6. **Add Claude Code hook example** to README (PostToolUse → auto-refresh)
