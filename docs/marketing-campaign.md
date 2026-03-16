# semantic-diff Marketing Campaign: Targeted Community Responses

> Strategy: Reply to specific community pain points with empathy-first responses. Lead with their problem, show the solution, invite feedback. No hard sell.

---

## Response Templates by Pain Point

---

### 1. "Claude Code has zero review features"

**Target:** HN comment by `embedding-shape` ([item?id=46207784](https://news.ycombinator.com/item?id=46207784))

**Original complaint:** *"Claude Code has absolutely zero features that help me review code or do anything else than vibe-coding and accept changes as they come in"*

**Response:**

> I built semantic-diff exactly for this. It's a terminal TUI that watches your Claude Code session and groups all the changed hunks by intent — so instead of scrolling through a flat list of modified files, you see groups like "Auth refactor" or "Test coverage" with the relevant hunks from across your codebase clustered together.
>
> It hooks into Claude Code's PostToolUse events, so every time Claude edits a file, the diff view auto-refreshes in a cmux split pane next to your session. No context switching, no external tools.
>
> It's open source (Rust + ratatui): https://github.com/alankyshum/semantic-diff
>
> Would love feedback on what review features you'd want most — per-hunk accept/reject? Inline comments? Something else?

---

### 2. "I have to use a diff tool to review changes in Claude Code"

**Target:** HN comment by `warmedcookie` ([item?id=44538495](https://news.ycombinator.com/item?id=44538495))

**Original complaint:** *"Cursor's Accept / Reject feature for each change it makes in each file is nice whereas I have to use a diff tool to review the changes in Claude Code"*

**Response:**

> This gap is what pushed me to build semantic-diff — a TUI that runs alongside Claude Code and gives you a Cursor-like review experience without leaving the terminal.
>
> It goes a step further than file-by-file diffs: it uses AI to group related hunks across files by intent (e.g. "API endpoint changes", "Error handling cleanup"), so you can review by *what the change means* rather than scrolling file-by-file.
>
> Hooks into Claude Code so it auto-refreshes on every edit. Syntax highlighting, collapse/expand, sidebar filtering — all in a ratatui TUI.
>
> https://github.com/alankyshum/semantic-diff
>
> Curious if the semantic grouping helps your review flow or if per-hunk accept/reject is the bigger need for you?

---

### 3. "You can't review code being generated faster than you can read it"

**Target:** HN comment by `trjordan` ([item?id=45424824](https://news.ycombinator.com/item?id=45424824)), in "Comprehension debt" thread (532 pts, 338 comments)

**Original complaint:** *"you can't review code being generated faster than you can read it"*

**Response:**

> This is the exact asymmetry I've been trying to address. You're right that line-by-line reading doesn't scale — but I think semantic grouping can help compress the review surface.
>
> I built semantic-diff, a terminal TUI that uses Claude/Copilot to cluster your git changes by intent. Instead of reviewing 40 changed files linearly, you see groups like "Database migration", "API contract changes", "Test updates" — each with only the relevant hunks pulled from across your codebase.
>
> It doesn't solve the comprehension problem entirely, but it lets you triage: focus on the high-risk semantic groups first, skim the mechanical ones.
>
> https://github.com/alankyshum/semantic-diff
>
> Would love to hear what would make this more useful for your workflow.

---

### 4. "The diff shows what changed, but not why"

**Target:** HN comment by `nachocoll` ([item?id=47322623](https://news.ycombinator.com/item?id=47322623))

**Original complaint:** *"The diff shows what changed, but not why, and not what the agent was doing when it generated that specific change."*

**Response:**

> Exactly — raw diffs are context-free. I've been working on semantic-diff, which tries to bridge that gap by using AI to group related hunks across files by inferred intent.
>
> So instead of seeing 15 files with red/green lines, you see named groups like "Refactor auth middleware" or "Add rate limiting" — each group pulls together the relevant hunks from different files that belong to the same logical change.
>
> It's not a full "explain why" yet, but the group labels give you the intent layer that raw diffs completely lack. Runs as a terminal TUI alongside Claude Code with auto-refresh.
>
> https://github.com/alankyshum/semantic-diff
>
> I'm considering adding per-group explanations — would that be useful, or is the grouping itself enough to orient your review?

---

### 5. "The files are shown in whatever order GitHub decides. No logical grouping."

**Target:** HN comment by `oddurmagnusson`, who built Gnosis to solve this

**Original complaint:** *"reviewing still sucks [...] the files are shown in whatever order GitHub decides. No logical grouping."*

**Response:**

> Love that you built Gnosis to solve this — the "guided walkthrough" approach is exactly right. I've been working on the same problem from the terminal side.
>
> semantic-diff is a ratatui TUI that uses AI to group git hunks by semantic intent. Similar philosophy to Gnosis (logical grouping > alphabetical file order), but designed for terminal-first workflows — runs in a tmux/cmux split pane alongside Claude Code.
>
> Key difference: it works at the hunk level, not file level. A single file's hunks can appear in different semantic groups if they serve different purposes. And it auto-refreshes via Claude Code hooks.
>
> https://github.com/alankyshum/semantic-diff
>
> Would be curious to compare notes on the grouping approaches. What's worked best in Gnosis?

---

### 6. "When it's in an IDE and I just have to tick accept I just end up getting over eager"

**Target:** HN comment by `martinald` ([item?id=44849538](https://news.ycombinator.com/item?id=44849538))

**Original complaint:** *"When it's in an IDE and I just have to tick accept I just end up getting over eager"*

**Response:**

> This is an underrated observation — the frictionless accept button is actually a bug, not a feature. It optimizes for speed when the bottleneck is comprehension.
>
> I built semantic-diff as a terminal TUI that adds a review step with *just enough* friction. It groups your git changes by semantic intent (using Claude or Copilot), so you review by meaning — "Auth changes", "Test additions", "Config updates" — rather than file-by-file.
>
> The intent grouping naturally slows you down on the right things: you can quickly skim mechanical changes while spending time on semantic groups that touch core logic.
>
> https://github.com/alankyshum/semantic-diff
>
> Terminal-native, no Electron, no browser. Curious if this kind of structured friction matches what you're looking for.

---

### 7. "Terminal tells you nothing. This shows you everything."

**Target:** `matt1398`, author of claude-devtools ([HN thread](https://news.ycombinator.com/item?id=46600362))

**Original complaint:** Built claude-devtools because Claude Code's terminal output hides what's actually happening.

**Response:**

> claude-devtools is great for visibility into Claude's execution — the token usage and agent tree view fill a real gap. I've been working on a complementary tool for the diff review side.
>
> semantic-diff is a terminal TUI that shows your actual code changes grouped by semantic intent. It hooks into Claude Code's PostToolUse events and auto-refreshes in a cmux split pane, so you always see a live, AI-grouped view of what's been modified.
>
> They pair well: claude-devtools for understanding *what Claude is doing*, semantic-diff for understanding *what Claude changed*.
>
> https://github.com/alankyshum/semantic-diff
>
> Would love to hear if you've thought about diff integration for claude-devtools, or if you'd see these as complementary.

---

### 8. "CLI based tools (eg. git diff) are just generally inferior"

**Target:** HN comment by `noodletheworld` (Graphite/Cursor thread)

**Original complaint:** *"The one place where it makes a difference is reviewing code, where most CLI based tools (eg. git diff) are just generally inferior to visual integrated code review tools."*

**Response:**

> Agreed that vanilla git diff is painful for review. But I think the gap isn't inherent to terminals — it's that nobody built the right TUI for it.
>
> I've been working on semantic-diff: a ratatui-based terminal diff viewer that uses AI to group changes by intent. Syntax highlighting, word-level inline diffs, collapse/expand, file tree sidebar with per-group filtering. Think of it as bringing the structured review experience of Cursor/GitHub to the terminal.
>
> The semantic grouping is the key differentiator — instead of 30 files in alphabetical order, you see "Database schema migration" (3 hunks across 2 files), "API endpoint updates" (5 hunks across 4 files), etc.
>
> https://github.com/alankyshum/semantic-diff
>
> Would this change your view on terminal-based review, or are there other IDE features you'd need?

---

### 9. "I accept all changes immediately and then commit after every change and then review the diff"

**Target:** HN comment by `fooster` ([item?id=44843263](https://news.ycombinator.com/item?id=44843263))

**Original complaint:** Workaround of committing after every AI change to create reviewable checkpoints.

**Response:**

> That commit-per-change workflow is clever but adds a lot of friction. I built semantic-diff to make the review step seamless without the extra commits.
>
> It's a terminal TUI that watches your uncommitted changes and groups them by semantic intent using Claude or Copilot. So after Claude Code makes a batch of edits, you immediately see them organized as "Refactored auth flow" (4 hunks, 3 files), "Added error handling" (2 hunks, 2 files), etc.
>
> Hooks into Claude Code's PostToolUse events for auto-refresh. Results are cached by diff hash so it's instant when nothing changed.
>
> https://github.com/alankyshum/semantic-diff
>
> Curious if this would replace your commit-per-change flow or if you'd use both together?

---

### 10. "Are diffs still useful for AI-assisted code changes?"

**Target:** HN Ask thread by `nuky` ([item?id=46619855](https://news.ycombinator.com/item?id=46619855)), 7 pts, 17 comments

**Original complaint:** Proposed snapshot comparison over raw diffs; concerned about reviewing probabilistic changes with probabilistic tools.

**Response:**

> Great question. I think diffs are still useful, but raw line-level diffs are not — they need a semantic layer on top.
>
> I've been building semantic-diff, which keeps the familiar diff format but adds AI-powered grouping: hunks from across your codebase are clustered by intent ("Auth changes", "Test coverage", "Config updates") so you can review by meaning rather than by file.
>
> Your point about AST-level behavioral signals is interesting — right now semantic-diff works at the hunk level with AI inference, but structural analysis could complement it. The grouping already helps with the "1000 red/green lines" problem by letting you triage which semantic clusters deserve deep review vs. a quick scan.
>
> https://github.com/alankyshum/semantic-diff
>
> Would AST-level change summaries within each semantic group be useful? Trying to figure out where the line is between "enough context" and "information overload."

---

### 11. "Most diff tools are focused on visualizing changes, not iterating on the change"

**Target:** HN comment by `llbbdd` on Deff thread

**Original complaint:** Wanted feedback loops between review tool and AI agents.

**Response:**

> This is a great insight — review and iteration should be one flow, not two separate tools. I've been building semantic-diff, which is currently focused on the visualization/grouping side (AI-powered hunk grouping by intent in a terminal TUI).
>
> The feedback loop you're describing — review a semantic group, leave a comment, have the agent fix it — is exactly where I want to take it next. Right now it hooks into Claude Code for auto-refresh, but the interaction is one-way (view only).
>
> https://github.com/alankyshum/semantic-diff
>
> Would "select a group → send feedback to Claude Code → see the updated diff" be the right interaction model? Or something closer to inline PR comments?

---

## Campaign Execution Notes

### Platform Priority
1. **Hacker News** — Highest signal, most engaged audience. Reply to existing threads where the tool is directly relevant.
2. **Reddit** (r/ClaudeAI, r/commandline, r/rust) — Post as a Show HN-style share with context about the problem.
3. **Twitter/X** — Quote-tweet or reply to developers sharing vibe coding frustrations.

### Timing
- Reply to **active threads** (< 7 days old) for visibility
- For older threads, create a new "Show HN" post referencing the pain points
- Best posting times: Tuesday–Thursday, 9–11am ET

### Tone Guidelines
- Lead with empathy: acknowledge their specific pain
- Show, don't sell: describe what the tool does concretely
- Invite feedback: always end with a specific question
- Be honest about limitations: don't overclaim
- Credit complementary tools: mention Difftastic, claude-devtools, etc. positively

### Metrics to Track
- GitHub stars after each post
- Inbound issues/discussions on the repo
- Replies and engagement on community posts
- `cargo install` download count on crates.io

### Show HN Post Draft

**Title:** Show HN: semantic-diff – Terminal TUI that groups git changes by intent using AI

**Body:**
> I built this because Claude Code has no built-in way to review what it changed. After watching AI agents modify 30+ files in a session, `git diff` just dumps a wall of red/green lines with no structure.
>
> semantic-diff is a Rust TUI (ratatui) that uses Claude or Copilot to group your uncommitted changes by semantic intent — "Auth refactor" (3 files), "Test coverage" (5 files), "Config cleanup" (2 files) — with syntax-highlighted, word-level diffs inside each group.
>
> Key features:
> - Hunk-level grouping (a single file's hunks can appear in different groups)
> - Auto-refreshes via Claude Code hooks (PostToolUse → SIGUSR1)
> - Runs in a cmux split pane alongside your coding session
> - Grouping cache keyed by diff hash (instant reload)
> - Works without AI too (falls back to ungrouped view)
>
> Install: `brew install alankyshum/tap/semantic-diff` or `cargo install semantic-diff`
>
> https://github.com/alankyshum/semantic-diff
>
> Would love feedback, especially on what would make the review flow more useful. Thinking about per-group explanations, accept/reject per hunk, and a feedback loop back to the coding agent.
