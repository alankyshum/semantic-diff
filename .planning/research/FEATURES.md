# Feature Research

**Domain:** Terminal diff viewer / code review TUI with semantic grouping
**Researched:** 2026-03-13
**Confidence:** MEDIUM-HIGH (based on direct GitHub repo analysis of diffnav, delta, difftastic, lazygit, gitui; training data for GitHub PR UI and Google Critique)

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume exist in any terminal diff viewer. Missing these = product feels broken or toy-like.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Syntax-highlighted diff output | Every modern diff tool (delta, diffnav, bat) does this. Uncolored diffs feel like raw `git diff` | MEDIUM | Use tree-sitter for language-aware highlighting. Delta set the bar here. |
| Unified diff view (inline +/- lines) | The fundamental diff format. Every tool supports it. PROJECT.md says this is v1. | LOW | Parse unified diff format from `git diff` output |
| File-level navigation (j/k between files) | diffnav, lazygit, gitui all have this. Users expect vim-like movement. | LOW | j/k/up/down to move between files in the file list |
| File tree / file list sidebar | diffnav has it, GitHub PR UI has it, lazygit has it. Standard way to navigate multi-file diffs. | MEDIUM | Show changed files organized in a tree or flat list |
| Expand/collapse files | diffnav and GitHub PR UI both support this. Essential when reviewing many files. | LOW | Toggle individual file diffs open/closed |
| Expand/collapse hunks | GitHub PR UI supports this. Important for large file diffs with many change regions. | LOW | Toggle individual hunks within a file |
| Change statistics (insertions/deletions) | diffnav shows "+N/-M" per file, GitHub shows per-file and total. Quick signal for change magnitude. | LOW | Parse from diff headers, display next to file names |
| Keyboard-driven navigation | Every TUI tool is keyboard-first. Mouse is secondary. vim-like bindings are expected. | LOW | j/k/Enter/q/Esc at minimum. Ctrl-d/Ctrl-u for page scroll. |
| Color-coded additions/deletions | Green for adds, red for deletes is universal convention. | LOW | Standard ANSI colors, configurable via theme |
| Scroll within file diffs | When a file diff is longer than the viewport, scroll must work. All TUIs handle this. | LOW | Vertical scrolling with j/k or Ctrl-d/Ctrl-u when focused on diff pane |
| Quit / exit cleanly | q to quit is standard. Must not leave terminal in broken state. | LOW | ratatui handles terminal restore on exit |
| Line numbers | delta, diffnav (via delta) show line numbers. Important for referencing specific changes. | LOW | Show old and new line numbers in gutter |

### Differentiators (Competitive Advantage)

Features no existing terminal tool offers, or that this project does uniquely well. This is where semantic-diff wins.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| AI-powered semantic grouping | **The core differentiator.** No terminal diff tool groups files by semantic meaning ("refactored auth", "added tests", "updated configs"). GitHub PR UI doesn't do this. Google Critique doesn't do this. This is genuinely novel. | HIGH | Requires LLM call via clauded. Must be async -- show ungrouped first, regroup when response arrives. |
| Collapsible semantic groups | Once files are grouped by meaning, users can expand/collapse entire groups. This is the "why" layer on top of the "what" layer. No existing tool has this. | MEDIUM | Depends on semantic grouping. Tree widget with group nodes containing file nodes. |
| Real-time hook-triggered refresh | Diff updates automatically when Claude Code edits files via PostToolUse hooks. No manual refresh needed. No polling. Unique to the Claude Code workflow. | MEDIUM | IPC mechanism (stdin pipe, Unix socket, or file watch on a trigger file). Must merge new diff state without losing scroll/collapse state. |
| cmux split-pane integration | Auto-opens as a right-side pane alongside Claude Code. The diff viewer is always there, not something you switch to. | MEDIUM | Shell script / hook that calls `cmux surface.split` to spawn the TUI. |
| Summary header per semantic group | Show "3 files, +45/-12" per semantic group, not just per file. Gives quick understanding of the scope of each logical change. | LOW | Aggregate stats from files within each group. Depends on semantic grouping. |
| Progressive enhancement (ungrouped -> grouped) | Show the diff immediately with files in path order, then animate/transition to semantic groups when LLM responds. No blocking on AI. | HIGH | Requires careful state management: preserve user's scroll/collapse state during re-layout. This is the hardest UX problem. |
| Word-level diff highlighting | delta does this with Levenshtein. Shows exactly which words/tokens changed within a line, not just "this line changed." | MEDIUM | Implement word-level diffing algorithm. Significant visual improvement over line-level diffs. |
| Search / filter files | diffnav has "go to file" (t key). Useful when many files changed. | LOW | Fuzzy search over file paths in the sidebar. |

### Anti-Features (Commonly Requested, Often Problematic)

Features to explicitly NOT build. These pull the project away from its core value.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Side-by-side diff view (v1) | diffnav defaults to it, delta supports it. Looks professional. | Requires significant horizontal space. In a cmux split pane, you have ~80 columns -- not enough for side-by-side. PROJECT.md explicitly defers this. | Unified inline diff for v1. Side-by-side can be v2 for wide terminals. |
| Git operations (stage, commit, push) | lazygit and gitui do this. Natural to want "full git TUI." | Massive scope expansion. This is a read-only diff viewer for monitoring Claude Code. Adding write operations changes the entire product category. | Stay read-only. Users already have lazygit/gitui for git ops. |
| File editing / opening in $EDITOR | diffnav has "open in $EDITOR" (o key). Seems useful. | The user is running Claude Code -- they're not manually editing files. Opening an editor would conflict with Claude's ongoing edits. | Could add "copy file path" (y key) for reference, but not open editor. |
| Filesystem watcher / polling | Seems natural for "real-time" updates. | CPU-intensive, generates noise from intermediate states (partial writes), and fights with the hook-based architecture. PROJECT.md explicitly excludes this. | Hook-triggered refresh only. Clean, event-driven, no wasted cycles. |
| Merge conflict resolution | Some diff viewers add conflict markers / resolution UI. | Out of scope per PROJECT.md. This is a viewer, not a merge tool. | Show conflicts as regular diff content if present. Don't try to resolve them. |
| Configurable themes / color schemes | Power users want customization. delta has 20+ stylable elements. | Premature for a v1 personal tool. Configuration is a maintenance burden and delays shipping. | Hardcode a good dark theme. Add theming in v2 if needed. |
| Structural / AST-based diffing | difftastic does this well. Shows "real" changes ignoring formatting. | Extremely complex to implement (tree-sitter parsing + graph algorithm). difftastic exists and does it better than we could. | Use line-level diff with word-level highlighting. Good enough for monitoring changes. |
| Mouse support | Some TUI users prefer clicking. | Adds testing complexity. The primary user is a keyboard-focused developer watching a split pane. | Keyboard-only for v1. ratatui supports mouse if needed later. |
| Multi-repo / multi-worktree support | Power users might want to watch multiple repos. | Adds routing/multiplexing complexity. The hook targets one repo at a time. | One instance per repo. Spawn another pane if needed. |
| Diff against arbitrary commits | lazygit lets you diff any two commits. Useful for general code review. | This tool diffs working tree vs HEAD, specifically for monitoring in-progress Claude edits. Arbitrary commit diffing is a different use case. | Always diff working tree vs HEAD (or staged vs HEAD). |

## Feature Dependencies

```
[Diff parsing]
    +--requires--> [Syntax highlighting]
    +--requires--> [Line numbers]
    +--requires--> [Change statistics]
    +--requires--> [Unified diff rendering]

[File tree sidebar]
    +--requires--> [File-level navigation]
    +--requires--> [Expand/collapse files]

[Expand/collapse hunks]
    +--requires--> [Diff parsing] (must know hunk boundaries)

[Semantic grouping (LLM)]
    +--requires--> [Diff parsing] (needs file list + diff content to send to LLM)
    +--requires--> [File tree sidebar] (groups are rendered in the sidebar)

[Collapsible semantic groups]
    +--requires--> [Semantic grouping]
    +--requires--> [Expand/collapse files] (same collapse pattern, higher level)

[Summary header per group]
    +--requires--> [Semantic grouping]
    +--requires--> [Change statistics]

[Progressive enhancement (ungrouped -> grouped)]
    +--requires--> [Semantic grouping]
    +--requires--> [File tree sidebar] (must re-layout without losing state)

[Hook-triggered refresh]
    +--requires--> [Diff parsing] (re-parse on each trigger)
    +--independent--> [Semantic grouping] (can refresh diff without re-grouping)

[Word-level diff highlighting]
    +--requires--> [Diff parsing]
    +--enhances--> [Unified diff rendering]

[Search / filter files]
    +--requires--> [File tree sidebar]
```

### Dependency Notes

- **Semantic grouping requires diff parsing:** The LLM needs to know which files changed and ideally some context about the changes to create meaningful groups.
- **Progressive enhancement is the hardest feature:** It requires the file tree to re-organize from flat/path-order into semantic groups while preserving user state (scroll position, which files are expanded). This should be a separate phase from basic semantic grouping.
- **Hook-triggered refresh is independent of semantic grouping:** The diff can refresh immediately while semantic grouping runs as a separate async process. This decoupling is critical for perceived performance.
- **Word-level highlighting enhances but doesn't block v1:** Line-level diffs are usable. Word-level is polish.

## MVP Definition

### Launch With (v1)

Minimum viable product -- what's needed to validate the concept of "semantic diff viewer in a cmux pane."

- [ ] **Diff parsing and unified rendering** -- the core function: parse `git diff` output and render it
- [ ] **Syntax highlighting** -- without this, users will just use `git diff | delta`
- [ ] **File tree sidebar** -- navigate between changed files
- [ ] **Expand/collapse files and hunks** -- manage information density
- [ ] **Keyboard navigation** -- j/k/Enter/q, Ctrl-d/Ctrl-u for scroll
- [ ] **Line numbers and change statistics** -- basic orientation info
- [ ] **Hook-triggered refresh** -- the diff updates when Claude edits files (this validates the real-time monitoring use case)

### Add After Validation (v1.x)

Features to add once the core TUI is working and the hook integration is proven.

- [ ] **Semantic grouping via clauded** -- add when the basic viewer is stable and you know the LLM call latency/reliability
- [ ] **Collapsible semantic groups** -- add alongside semantic grouping
- [ ] **Summary header per group** -- low cost once grouping exists
- [ ] **cmux auto-split integration** -- add when the viewer binary is reliable enough to auto-launch
- [ ] **Word-level diff highlighting** -- visual polish, add when core rendering is solid

### Future Consideration (v2+)

Features to defer until the tool is daily-driven and pain points emerge.

- [ ] **Progressive enhancement (smooth ungrouped -> grouped transition)** -- complex UX, defer until grouping UX is validated
- [ ] **Side-by-side diff view** -- only if terminal width permits; needs wide terminal detection
- [ ] **Search / filter files** -- useful at scale (20+ files changed), defer until that's a real problem
- [ ] **Theming / configuration** -- defer until the hardcoded theme proves insufficient
- [ ] **Mouse support** -- defer unless keyboard-only proves painful

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Diff parsing + unified rendering | HIGH | MEDIUM | P1 |
| Syntax highlighting | HIGH | MEDIUM | P1 |
| File tree sidebar | HIGH | MEDIUM | P1 |
| Expand/collapse files | HIGH | LOW | P1 |
| Expand/collapse hunks | MEDIUM | LOW | P1 |
| Keyboard navigation (vim-like) | HIGH | LOW | P1 |
| Line numbers | MEDIUM | LOW | P1 |
| Change statistics | MEDIUM | LOW | P1 |
| Hook-triggered refresh | HIGH | MEDIUM | P1 |
| Semantic grouping (LLM) | HIGH | HIGH | P2 |
| Collapsible semantic groups | HIGH | MEDIUM | P2 |
| Group summary headers | MEDIUM | LOW | P2 |
| cmux auto-split integration | MEDIUM | LOW | P2 |
| Word-level diff highlighting | MEDIUM | MEDIUM | P2 |
| Progressive enhancement transition | MEDIUM | HIGH | P3 |
| Side-by-side diff view | LOW | HIGH | P3 |
| Search / filter files | LOW | LOW | P3 |
| Theming / configuration | LOW | MEDIUM | P3 |

**Priority key:**
- P1: Must have for launch (validates the concept)
- P2: Should have, add after core is working (enables the differentiating value)
- P3: Nice to have, future consideration

## Competitor Feature Analysis

| Feature | diffnav | delta | lazygit | gitui | difftastic | GitHub PR UI | semantic-diff (ours) |
|---------|---------|-------|---------|-------|------------|-------------|---------------------|
| Syntax highlighting | Yes (via delta) | Yes (bat themes) | Limited | Themes only | Yes (tree-sitter) | Yes | Yes (tree-sitter) |
| Unified diff | Yes (toggle) | Yes | Yes | Yes | No (structural) | Yes (default) | Yes (v1 default) |
| Side-by-side | Yes (default) | Yes | No | No | Yes (default) | Yes (toggle) | No (v2) |
| File tree/list | Yes | No (pager only) | Yes | Yes | No | Yes | Yes |
| Expand/collapse files | Yes | No | Yes | Yes | No | Yes | Yes |
| Expand/collapse hunks | No | No | No | No | No | Yes | Yes |
| Word-level highlighting | Via delta | Yes (Levenshtein) | No | No | Yes (structural) | Yes | v1.x |
| Line numbers | Via delta | Yes | Yes | Yes | Yes | Yes | Yes |
| Change stats per file | Yes | No | Yes | Yes | No | Yes | Yes |
| Keyboard navigation | Yes (vim) | N/A (pager) | Yes (vim) | Yes (vim) | N/A | N/A (web) | Yes (vim) |
| Git operations | No | No | Yes (full) | Yes (full) | No | Yes (review) | No (read-only) |
| Semantic grouping | No | No | No | No | No | No | **Yes (unique)** |
| Real-time refresh | No | No | Manual | Manual | No | Webhook/poll | **Yes (hook-triggered)** |
| LLM integration | No | No | No | No | No | Copilot review | **Yes (clauded)** |
| Search/filter files | Yes | No | Yes | Yes | No | Yes | v2 |

**Key insight:** No existing tool -- terminal or web -- offers semantic grouping of diffs. GitHub Copilot does AI-powered code review (suggesting changes), but does not group files by semantic meaning. This is a genuinely novel feature in the diff viewer space.

## Sources

- diffnav GitHub repo: https://github.com/dlvhdr/diffnav (MEDIUM confidence -- direct repo analysis)
- delta GitHub repo: https://github.com/dandavison/delta (MEDIUM confidence -- direct repo analysis)
- lazygit GitHub repo: https://github.com/jesseduffield/lazygit (MEDIUM confidence -- direct repo analysis)
- gitui GitHub repo: https://github.com/extrawurst/gitui (MEDIUM confidence -- direct repo analysis)
- difftastic GitHub repo: https://github.com/Wilfred/difftastic (MEDIUM confidence -- direct repo analysis)
- GitHub PR review UI features: based on training data (LOW confidence -- not directly verified with current docs)
- Google Critique features: based on training data (LOW confidence -- internal tool, no public docs)

---
*Feature research for: Terminal diff viewer / code review TUI with semantic grouping*
*Researched: 2026-03-13*
