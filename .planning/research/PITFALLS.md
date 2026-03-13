# Pitfalls Research

**Domain:** Rust TUI diff viewer with async LLM integration and terminal multiplexer embedding
**Researched:** 2026-03-13
**Confidence:** MEDIUM (ratatui patterns verified via official docs; git2 edge cases verified via docs.rs; subprocess/cmux pitfalls based on training data + domain reasoning)

## Critical Pitfalls

### Pitfall 1: Blocking the Render Loop with Synchronous clauded Calls

**What goes wrong:**
The `clauded` CLI call for semantic grouping takes 2-10+ seconds. If called synchronously in the main event/render loop, the entire TUI freezes -- no keyboard input processed, no visual feedback, no way to quit. The user sees a hung terminal pane.

**Why it happens:**
Ratatui uses immediate-mode rendering where the main loop handles both events and drawing. Developers new to TUI apps treat the LLM call like a normal function call, forgetting that nothing renders while that function blocks. The PROJECT.md already identifies async grouping as a requirement, but the implementation is where it goes wrong -- spawning a `std::process::Command` instead of `tokio::process::Command`, or awaiting the future in the wrong place.

**How to avoid:**
- Use `tokio::process::Command` with `.spawn()` to run clauded as a non-blocking child process.
- Communicate results back to the main loop via an `mpsc` channel or `tokio::sync::watch`.
- The render loop should poll the channel on each tick, never `.await` the subprocess directly.
- Show a "Grouping..." spinner/indicator while the LLM call is in-flight.
- Pattern: Event loop reads from a `tokio::select!` over (user input channel, subprocess result channel, tick interval).

**Warning signs:**
- TUI becomes unresponsive for several seconds after a hook triggers a refresh.
- `q` key does not work while waiting for grouping.
- No visual change between "diff loaded" and "grouped diff loaded."

**Phase to address:**
Phase 1 (core architecture). The async event loop design must be established before any clauded integration. Build the channel-based architecture even with a mock/stub LLM response first.

---

### Pitfall 2: Ratatui State Desync -- Stale UI After Async Updates

**What goes wrong:**
Ratatui is immediate-mode: the UI only updates when you call `terminal.draw()`. If the async grouping result arrives between renders (or the render loop is tick-based at a low frequency), the user sees stale ungrouped data for an unpredictable time. Worse, if the state struct is mutated from the async task while the render is reading it, you get data races (in unsafe code) or confusing partial updates (in safe code with interior mutability).

**Why it happens:**
Developers assume "updating the state struct" automatically refreshes the display, carrying over mental models from reactive frameworks (React, Elm with signals). In ratatui, nothing happens until the next `terminal.draw()` call. The render loop needs to be woken up when async data arrives.

**How to avoid:**
- Use a single-threaded main loop that owns all state. Async tasks send messages; the main loop applies them.
- After receiving a message from the async channel, immediately trigger a re-render (not waiting for the next tick).
- Keep the tick rate reasonable (100-250ms) as a fallback, but prefer event-driven wakeups.
- Never share `&mut AppState` across threads. The main loop is the sole owner.

**Warning signs:**
- UI updates feel "laggy" even though the data arrived quickly.
- Occasional panics or inconsistent display when grouping results arrive during a render.
- Tests pass but manual testing shows brief flicker of ungrouped then grouped content.

**Phase to address:**
Phase 1 (core architecture). Define the `AppState` ownership model and message-passing pattern before building any widgets.

---

### Pitfall 3: Git Diff Parser Fails on Renames, Binary Files, and Empty Diffs

**What goes wrong:**
The diff parser handles simple adds/removes but breaks on: (1) renamed files (shows as delete + add instead of rename), (2) binary files (tries to parse binary content as text, produces garbage or panics on invalid UTF-8), (3) empty files or files with only mode changes, (4) submodule changes, (5) symlink changes.

**Why it happens:**
Developers test against their own repos which typically have clean text-file diffs. Claude Code frequently renames files, creates binary lock files, or changes file permissions -- exactly the edge cases that get skipped. The git2 `Diff` API has separate callback paths for binary vs text content (`binary_cb` vs `hunk_cb`/`line_cb` in `foreach()`), and skipping the binary callback silently drops files from the view.

**How to avoid:**
- Use git2's `Diff::foreach()` with ALL four callbacks (file, binary, hunk, line) -- never skip the binary callback.
- Call `diff.find_similar(Some(&mut DiffFindOptions::new().renames(true).copies(true)))` BEFORE iterating deltas. Without this call, renames appear as separate delete/add pairs.
- Handle `Delta::Renamed`, `Delta::Copied`, `Delta::Typechange` status codes explicitly.
- For binary files, display "[binary file changed]" with file size delta rather than attempting to show content.
- Test with a fixture repo containing: renamed files, binary files (images, lock files), empty files, mode-only changes, submodule pointer changes.

**Warning signs:**
- A renamed file shows up as two entries (one red deletion, one green addition) instead of one rename entry.
- The app panics with "invalid UTF-8" or displays garbled content.
- Some changed files from `git status` are missing from the TUI.
- File count in summary header does not match `git diff --stat`.

**Phase to address:**
Phase 2 (diff parsing). Build a comprehensive test suite with edge-case fixtures before moving to rendering.

---

### Pitfall 4: Terminal Cleanup Failure Leaves Raw Mode Active After Crash

**What goes wrong:**
If the app panics or is killed (SIGKILL, OOM), the terminal remains in raw mode with the alternate screen active. The user's shell becomes unusable -- no echo, no line editing, garbled output. Since this runs in a cmux pane, the entire pane becomes broken and the user must manually run `reset` or close the pane.

**Why it happens:**
Ratatui switches the terminal to raw mode and alternate screen on startup. The cleanup (restoring normal mode) happens in `Drop` impls or explicit cleanup code. Panics that unwind will run Drop, but `panic=abort` in Cargo.toml (common for smaller binaries) skips Drop entirely. SIGKILL always skips cleanup.

**How to avoid:**
- Install a custom panic hook that restores the terminal BEFORE printing the panic info: `std::panic::set_hook(Box::new(|info| { restore_terminal(); eprintln!("{info}"); }))`.
- Use `color_eyre` or similar for panic/error hooks that handle terminal restoration.
- Do NOT set `panic = "abort"` in Cargo.toml.
- Handle SIGTERM and SIGINT via `tokio::signal` or `signal-hook` crate to run cleanup.
- For SIGKILL (unrecoverable), document that `reset` command or closing the cmux pane is the recovery path.

**Warning signs:**
- During development, any panic leaves the terminal in a broken state.
- Users report having to close and reopen cmux panes.
- The hook script that launches semantic-diff does not include a cleanup trap.

**Phase to address:**
Phase 1 (scaffolding). Panic hook and signal handling must be the very first thing set up, before any TUI rendering code.

---

### Pitfall 5: Hook-Triggered Refresh Creates Race Conditions with In-Flight LLM Calls

**What goes wrong:**
Claude Code fires PostToolUse hooks rapidly (multiple Edit/Write calls in quick succession). Each hook triggers a refresh, which triggers a new `clauded` call. Multiple clauded processes run concurrently, and their results arrive out of order. The UI flickers between different grouping states, or an old grouping overwrites a newer one.

**Why it happens:**
The hook fires per-tool-call, and Claude Code can make dozens of edits in seconds during a refactoring session. Without debouncing or cancellation, each hook spawns an independent clauded process. The last one to finish "wins," which is not necessarily the most recent one.

**How to avoid:**
- Debounce incoming refresh signals: wait 500ms after the last hook trigger before starting a new diff + grouping cycle.
- Cancel in-flight clauded processes when a new refresh arrives (kill the child process).
- Tag each clauded request with a monotonic sequence number; discard results whose sequence number is less than the latest request.
- The diff itself (which is fast, <100ms) can run on every trigger, but the clauded call should be debounced.
- Show a "refreshing..." indicator when a debounce timer is active.

**Warning signs:**
- UI flickers between different grouping arrangements during rapid edits.
- Multiple `clauded` processes visible in `ps aux` simultaneously.
- Stale grouping displayed after the diff has already changed.
- High CPU/memory from accumulated clauded processes.

**Phase to address:**
Phase 3 (hook integration). Build the debounce/cancellation mechanism as part of the hook handler, not retrofitted later.

---

### Pitfall 6: clauded Subprocess Hangs or Fails Silently

**What goes wrong:**
The `clauded` daemon may not be running, may be rate-limited, may hang indefinitely, or may return malformed output. The app waits forever for a response, or crashes trying to parse unexpected output. If clauded requires authentication or the daemon socket is stale, the subprocess exits with an error that is swallowed.

**Why it happens:**
Developers test with a healthy clauded daemon and never simulate failure modes. The `clauded` CLI is a relatively new tool with its own lifecycle management. Its output format may change between versions. There is no formal API contract -- you are shelling out to a CLI tool.

**How to avoid:**
- Set a timeout on the clauded subprocess (10-15 seconds). Use `tokio::time::timeout` wrapping the process output.
- Check the exit code. Non-zero means failure; fall back to ungrouped display.
- Validate the output format defensively. Expect JSON (or whatever format is used) and handle parse errors gracefully.
- If clauded is not available at all (not installed, daemon not running), detect this at startup and disable semantic grouping with a status indicator ("grouping unavailable").
- Log clauded stderr for debugging but do not display it to the user.

**Warning signs:**
- App hangs when clauded daemon is not running.
- App crashes with deserialization errors after a clauded version update.
- No indication to the user that grouping failed -- just ungrouped diffs forever.

**Phase to address:**
Phase 3 (LLM integration). Build the clauded integration with timeout, fallback, and health-check from day one.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Shelling out to `git diff` CLI instead of using git2 | Simpler initial implementation, familiar output | Parsing unified diff text is fragile, loses structured data (rename info, binary flags), slower for large diffs | Never -- git2 is the right choice for a Rust project; the structured API prevents an entire class of parsing bugs |
| Single monolithic `AppState` struct | Fast to prototype | Becomes unwieldy as features are added (sidebar, groups, expansion state, scroll state, search state). Hard to test individual components | MVP only -- refactor into component-owned state by Phase 2 |
| Hardcoded clauded command path | Works on developer machine | Breaks on other machines or if clauded moves. No way to configure alternative LLM backends | MVP only -- use a config option or PATH lookup by Phase 3 |
| Rendering every line of every diff on each frame | Simple rendering code | Unacceptable performance for large diffs (1000+ lines). Frame time exceeds 16ms | Phase 1 only -- implement viewport culling (only render visible lines) in Phase 2 |
| String-based IPC with hook (writing to a file or pipe) | Quick to implement | Race conditions with concurrent writes, no structured protocol, hard to extend | Phase 1 for prototyping -- move to a proper signal mechanism (Unix socket or signal) by Phase 3 |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| cmux split-pane | Assuming the pane size is fixed at creation time | Listen for terminal resize events (`crossterm::event::Event::Resize`) and re-layout. cmux panes can be resized by the user at any time |
| cmux split-pane | Not handling pane close gracefully | Trap the case where the parent cmux session ends or the pane is force-closed. The app should exit cleanly on SIGHUP |
| Claude Code hooks | Assuming the hook runs in the same directory as the repo | Hooks may run from a different CWD. Always pass the absolute repo path as an argument to semantic-diff |
| Claude Code hooks | Blocking the hook script (which blocks Claude Code) | The hook script must launch semantic-diff in the background (`&`) or be non-blocking. A slow hook delays Claude Code's next action |
| git2 repo opening | Opening the repo once at startup and caching the handle | The repo state becomes stale. Re-open or refresh the repo on each diff cycle to pick up new commits and index changes |
| clauded output | Assuming a fixed JSON schema | Version the expected schema. Parse defensively with `serde` using `#[serde(default)]` and `Option<T>` for fields that may not exist |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Re-rendering the entire diff on every frame | High CPU usage (>30% idle), visible lag on resize | Only render the visible viewport. Calculate which lines are in view and skip the rest | Diffs larger than ~500 lines |
| Calling `git2::Diff::find_similar()` on every refresh | 100-500ms delay per refresh for repos with many files | Cache rename detection results; only recompute when the diff actually changes (compare diff stats or hash) | Repos with >50 changed files |
| Syntax highlighting every line on every frame | Frame time exceeds 16ms, noticeable stutter | Pre-compute syntax highlighting when diff is loaded, cache the styled spans. Only re-highlight on diff change, not on scroll | Files larger than ~200 lines each |
| Spawning a new clauded process per hook event | Multiple processes fighting for CPU/memory, results arriving out of order | Debounce hook events (500ms), cancel in-flight requests, one clauded process at a time | Rapid edit sequences (>3 edits in 2 seconds) |
| Loading entire diff into memory as owned Strings | Memory usage grows linearly with diff size | Use `Cow<str>` or reference the git2 diff buffers directly where possible. For display, only materialize strings for the visible viewport | Diffs larger than ~10MB (e.g., lock file regeneration) |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| No loading state while clauded processes | User sees ungrouped diff, then it suddenly rearranges. Disorienting, feels broken | Show a clear "Grouping changes..." indicator. When grouping arrives, animate or highlight the transition |
| Losing scroll position on refresh | User is reading a specific file, hook fires, entire view resets to top | Preserve the current scroll position and focused file across refreshes. If the focused file is still in the diff, keep it focused |
| Collapsing all groups on refresh | User expanded specific groups to review, refresh collapses everything | Track expansion state by group identity (semantic label or file set), not by index. Preserve expansion state across refreshes |
| No visual difference between "no changes" and "loading" | User cannot tell if the tool is working or if there are genuinely no changes | Show explicit states: "Waiting for changes...", "Loading diff...", "No changes detected" |
| Cramped display in narrow cmux pane | Diff content is truncated or wrapped poorly in a pane that is only 60 columns wide | Design for minimum 60-column width. Test at various pane sizes. Use horizontal scrolling for long lines rather than wrapping |

## "Looks Done But Isn't" Checklist

- [ ] **Diff parsing:** Often missing rename detection -- verify that `find_similar()` is called and `Delta::Renamed` is handled
- [ ] **Diff parsing:** Often missing binary file handling -- verify binary callback is implemented and binary files show a placeholder
- [ ] **Terminal cleanup:** Often missing panic hook -- verify that a panic does not leave the terminal in raw mode
- [ ] **Resize handling:** Often missing terminal resize events -- verify that resizing the cmux pane re-layouts correctly
- [ ] **Hook integration:** Often missing background launch -- verify the hook script does not block Claude Code
- [ ] **Scroll state:** Often missing preservation across refresh -- verify scroll position survives a hook-triggered update
- [ ] **Error display:** Often missing clauded failure indication -- verify the UI shows a message when grouping fails
- [ ] **Empty state:** Often missing "no changes" screen -- verify the app does not show an empty screen with no explanation
- [ ] **Unicode handling:** Often missing wide character support -- verify CJK characters, emoji in file paths, and non-ASCII content render correctly
- [ ] **Large diffs:** Often missing viewport culling -- verify a 5000-line diff does not freeze the UI

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Terminal left in raw mode | LOW | Run `reset` command, or close and reopen the cmux pane. Add panic hook to prevent recurrence |
| Stale grouping displayed | LOW | Force refresh (keybinding). Fix by implementing sequence-numbered requests |
| Multiple clauded processes accumulated | LOW | Kill orphan processes. Fix by implementing proper cancellation |
| Diff parser crashes on binary | MEDIUM | Add binary callback. Requires refactoring the diff iteration to use all four `foreach()` callbacks |
| Monolithic AppState becomes unmaintainable | HIGH | Requires architectural refactor to component-based state. Hard to do incrementally if widgets directly access top-level fields |
| Scroll position lost on every refresh | MEDIUM | Requires adding identity-based tracking (by file path, group label) to scroll/focus state. Retrofit is messy if the scroll model was index-based |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Blocking render loop with sync clauded | Phase 1 (architecture) | Verify render loop uses `tokio::select!` over channels. Mock clauded with a `sleep(5s)` -- TUI must remain responsive |
| State desync after async updates | Phase 1 (architecture) | Verify message-passing pattern. Send a mock async message -- UI updates within one tick |
| Terminal cleanup failure | Phase 1 (scaffolding) | Verify panic hook exists. `panic!()` in dev -- terminal must restore correctly |
| Diff parser edge cases | Phase 2 (diff parsing) | Test suite with fixture repo containing renames, binaries, empty files, mode changes, submodules |
| Hook race conditions | Phase 3 (hook integration) | Fire 10 rapid hook signals -- only one clauded process should be active, UI should not flicker |
| clauded failure handling | Phase 3 (LLM integration) | Kill clauded daemon, trigger refresh -- UI should show "grouping unavailable" and display ungrouped diff |
| cmux resize handling | Phase 2 (rendering) | Resize cmux pane while app is running -- layout should adapt without crash |
| Scroll position preservation | Phase 2 (navigation) | Expand a file, scroll to it, trigger refresh -- focus should remain on the same file |
| Performance on large diffs | Phase 2 (rendering) | Generate a 5000-line diff -- frame time must stay under 16ms, no visible stutter |

## Sources

- Ratatui official docs: rendering concepts, Elm architecture pattern, application recipes (https://ratatui.rs/concepts/, https://ratatui.rs/recipes/apps/) -- MEDIUM confidence
- Ratatui GitHub discussions: common user pain points around CPU usage, event handling, layout (https://github.com/ratatui/ratatui/discussions) -- MEDIUM confidence
- git2 docs.rs: Diff, DiffOptions, DiffFindOptions API documentation (https://docs.rs/git2/latest/git2/) -- HIGH confidence
- Ratatui immediate-mode rendering model and its implications -- verified via official docs, HIGH confidence
- clauded subprocess lifecycle and failure modes -- based on PROJECT.md constraints and general subprocess management patterns, LOW confidence (clauded is a novel tool with limited public documentation)
- cmux integration patterns -- based on PROJECT.md context and terminal multiplexer conventions, LOW confidence (cmux-specific docs not verified)

---
*Pitfalls research for: Rust TUI diff viewer with async LLM integration*
*Researched: 2026-03-13*
