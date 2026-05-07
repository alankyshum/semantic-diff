# semantic-diff cache/code-quality audit

Audit target: `/Users/kshum/.local/share/semantic-diff-src/`  
Focus: recently-added per-section disk cache, whole-result replay, and related code quality.

## 1. Executive summary

- 🔴 **Must-fix: whole-result replay has no invalidation beyond `(diff,title)` and `status == complete`.** `main.rs` replays any complete `result.json` at the deterministic dir without checking `metadata.tool_version`, `schema_version`, skill file hashes, prompt version, or current review source (`crates/semantic-diff-cli/src/main.rs:143-181`). This can silently serve stale reports forever after a binary upgrade, prompt change, schema change, or skill change.
- 🔴 **Must-fix: replayed CLI runs leave SSE clients subscribed to a channel that never emits.** The server is started and a notifier is preregistered before the replay check (`main.rs:115-135`), then replay waits for Ctrl+C without sending `complete` (`main.rs:157-177`). The frontend always subscribes after loading the document (`web/src/routes/r/[id]/+page.svelte:99-127`), and the SSE handler keeps the stream alive indefinitely (`crates/semantic-diff-cli/src/server.rs:297-323`).
- 🟡 **Should-fix: the old grouping cache module is dead in this workspace.** `semantic_diff_core::cache` is still exported (`crates/semantic-diff-core/src/lib.rs:4`) and defines `diff_hash/load/save` (`crates/semantic-diff-core/src/cache.rs:28-78`), but workspace search found no callers other than its own unit tests (`cache.rs:143-155`). Remove it or explicitly keep it as a public compatibility API.
- 🟡 **Should-fix: `ReviewCache`, `GroupReview`, and `group_content_hash` are orphaned old-cache remnants.** They are defined in `review/mod.rs:61-123`; workspace search found no construction/callers. `ResultDocument` now owns serializable `GroupReviewEntry` and `SectionEntry` (`result.rs:84-92`, `result.rs:291-349`), and the orchestrator uses those plus `CachedSection` (`orchestrator.rs:12`, `orchestrator.rs:178-188`, `orchestrator.rs:225-228`).
- 🟡 **Should-fix: cache key/hash logic is duplicated and inconsistent.** There are at least four unrelated hash identities: old `DefaultHasher` diff hash (`cache.rs:28-31`), orphaned `DefaultHasher` group hash (`review/mod.rs:123-134`), actual per-group blake3 16-hex hash (`result.rs:293-310`), and run-id blake3 8-hex hash duplicated in `ResultDocument::new`, CLI, and server (`result.rs:248-253`, `main.rs:103-109`, `server.rs:688-695`).
- 🟡 **Should-fix: skill file hashing is duplicated.** `orchestrator::collect_skill_files` hashes the skill file (`orchestrator.rs:333-350`) while `review::skill_hash` separately does the same (`review/mod.rs:237-248`). This makes provenance and cache invalidation easy to drift.
- 🟡 **Should-fix: per-section cache invalidates on skill body only, not prompt/tool version.** `CachedReview` stores `source` and `skill_hash` (`review/mod.rs:185-201`) and `load_sections_from_disk` validates only content hash, source path, and skill hash (`review/mod.rs:301-323`). A change to `build_review_prompt` (`review/llm.rs:135-144`) can replay stale section text.
- 🟢 **Nice-to-have: the UTF-8 boundary panic in `lint_markdown_mermaid` is fixed, but nearby byte-indexed helpers still rely on ASCII delimiters.** The main loop now advances by `ch.len_utf8()` (`review/mermaid_lint.rs:305-310`) and has a regression test (`review/mermaid_lint.rs:406-423`). Remaining slices in the same file are currently boundary-safe because their indices come from ASCII byte scans, but should be documented or converted to safer helpers during cleanup.
- 🟢 **Nice-to-have: cache write failures are mostly silent.** `save_sections_to_disk` drops errors from `create_dir_all`, serialization, write, and rename (`review/mod.rs:260-277`), and `delete_review_from_disk` ignores removal errors (`review/mod.rs:327-330`). Best-effort caching is fine, but warnings would make broken cache dirs debuggable.
- 🟢 **Nice-to-have: trivial clippy cleanup remains.** `ParseOptions` is cloned before first parse (`review/mermaid_lint.rs:121-123`), but the user-reported clippy issue says it is `Copy`; remove the clone.

## 2. Dead code findings

### 2.1 Old grouping cache module is unused

- **Definition/export:** `crates/semantic-diff-core/src/cache.rs:28-78` defines `diff_hash`, `load`, and `save`; `crates/semantic-diff-core/src/lib.rs:4` exports `pub mod cache`.
- **Verified call sites:** workspace search for `cache::load`, `cache::save`, `cache::diff_hash`, and direct `load/save/diff_hash` matches found only the module definitions and tests (`cache.rs:143-155`), plus unrelated `config::load`/`Config::save` matches.
- **Why stale:** the current orchestrator always calls the LLM grouper directly (`orchestrator.rs:111-129`) and never calls the old cache before or after grouping.
- **Proposed action:** delete `crates/semantic-diff-core/src/cache.rs` and remove `pub mod cache` from `lib.rs:4`, unless `semantic-diff-core` intentionally promises this as an external public API. If keeping it, add a comment/test proving the intended external surface.

### 2.2 `ReviewCache` and `GroupReview` are unused

- **Definitions:** `GroupReview` is defined at `review/mod.rs:61-67`; `ReviewCache` and its LRU methods are defined at `review/mod.rs:69-120`.
- **Verified call sites:** workspace search found these names only in their definitions in `review/mod.rs`; no orchestrator/server/result usage.
- **Current replacement:** serializable result state is `GroupReviewEntry` + `SectionEntry` (`result.rs:38-92`), initialized by `ResultDocument::set_groups` (`result.rs:291-349`) and mutated through `ResultDocument::set_section` (`result.rs:354-372`). The orchestrator writes directly to that document (`orchestrator.rs:188`, `orchestrator.rs:256`).
- **Proposed action:** delete `GroupReview`, `ReviewCache`, `MAX_CACHED_REVIEWS`, and the `VecDeque` import (`review/mod.rs:9`) unless there is a planned in-memory UI cache. If it is planned, add `#[allow(dead_code)]` plus a comment with the intended owner.

### 2.3 `SectionState` is only a compatibility bridge now

- **Definition/export:** `SectionState` is defined at `review/mod.rs:36-50` and re-exported from the crate root at `lib.rs:11`.
- **Remaining use:** `result.rs:4` imports it only for `impl From<&SectionState> for SectionEntry` at `result.rs:46-55`. Workspace search found no construction of `SectionState::Loading/Ready/Error/Skipped` outside tests/comments.
- **Current replacement:** new result writes directly build `SectionEntry { state, content }` in `set_groups` and `set_section` (`result.rs:334-361`).
- **Proposed action:** delete `SectionState` and its `From` impl if there is no external API contract. Otherwise document it as a legacy public conversion type and add tests for the public conversion.

### 2.4 `group_content_hash` is unused and conflicts with the real group hash

- **Definition:** `review::group_content_hash` uses `DefaultHasher` and returns `u64` (`review/mod.rs:122-135`).
- **Verified call sites:** workspace search found no caller.
- **Conflict:** the actual cache key consumed by the orchestrator is `GroupEntry.content_hash`, a 16-char blake3 hex computed in `ResultDocument::set_groups` (`result.rs:293-310`) and copied into `group_hashes` (`orchestrator.rs:142-149`).
- **Proposed action:** delete `group_content_hash`. If a reusable hash helper is desired, move the `set_groups` blake3 algorithm into a single public helper returning the same `String` used by `GroupEntry.content_hash`.

### 2.5 Other touched-file public items

- `CachedReview` is public at `review/mod.rs:190-202`, but no code outside `review/mod.rs` constructs it; only `CachedSection` is imported by the orchestrator (`orchestrator.rs:12`). **Proposed action:** make `CachedReview` private unless cache-file schema is intended as public API.
- `lint_mermaid`, `lint_markdown_mermaid`, and `LintResult` are public/re-exported (`review/mod.rs:5`, `review/mermaid_lint.rs:48-76`, `review/mermaid_lint.rs:286-313`) and are used by the orchestrator via `review::lint_markdown_mermaid` (`orchestrator.rs:438-440`); keep.

## 3. Duplication findings

### 3.1 Cache layer overlap: whole-result replay shadows per-section cache for the common CLI path

**Flow verified:**

1. CLI computes the deterministic ID from `(diff,title)` (`main.rs:103-109`) and uses `default_output_dir(&preliminary_id)` when `--output` is absent (`main.rs:111-113`).
2. It starts the server and opens `/r/<id>` before orchestration (`main.rs:115-141`).
3. It then checks `output_dir/result.json`; if `--no-cache` is false, `--no-llm` is false, `--output` is absent, and `status == complete`, it returns without calling the orchestrator (`main.rs:143-181`).
4. The per-section cache only runs inside the orchestrator after grouping and `ResultDocument::set_groups` (`orchestrator.rs:138-172`).

**Conclusion:** per-section cache is **not fully redundant**, but it is shadowed for the default repeated-CLI scenario. It still fires when:

- the deterministic result is missing or not complete (`main.rs:157-170`, then `orchestrator.rs:160-172`),
- the user passes a custom `--output`, which explicitly disables replay (`main.rs:149-160`) but not per-section cache (`orchestrator.rs:160-172`),
- the run is launched through `POST /api/runs`, which does not implement whole-result replay and always spawns `orchestrator::run` (`server.rs:704-765`),
- a prior run has only some per-group entries cached; live tasks fill the rest and then persist (`orchestrator.rs:225-228`, `orchestrator.rs:312-325`).

**Recommendation:** do not delete the per-section cache solely because replay exists. Instead, choose one of these simpler designs:

- **Option A (smallest change):** keep per-section cache, but update comments/docs so it is described as a resume/custom-output/UI cache rather than the primary default-CLI cache. Add replay invalidation so stale whole-result replay does not permanently mask the per-section invalidation logic.
- **Option B (collapse layers):** remove whole-result replay and rely on per-section cache. This redoes diff parsing/grouping but preserves prompt/skill invalidation and yields cache-hit timings.
- **Option C (preferred if instant replay matters):** keep replay, but validate it against a shared `CacheKeyContext` before returning; otherwise fall through to orchestrator/per-section cache.

### 3.2 Run ID hashing duplicated in three places

- `ResultDocument::new` computes `blake3(raw_diff || title)[..8]` (`result.rs:248-253`).
- CLI computes the same `preliminary_id` (`main.rs:103-109`).
- Server computes the same for UI runs (`server.rs:688-695`).

**Proposal:** centralize in core, and use a char-safe prefix helper instead of raw string slicing:

```rust
// semantic_diff_core::result or ids module
pub fn result_id(raw_diff: &str, title: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(raw_diff.as_bytes());
    h.update(title.as_bytes());
    h.finalize().to_hex().as_str()[..8].to_string()
}
```

Because blake3 hex is ASCII, the slice is safe, but centralizing prevents drift and makes that invariant local. Then call it from `ResultDocument::new`, `main.rs`, and `server.rs`.

### 3.3 Group content hashing duplicated/inconsistent

- Old grouping cache uses `DefaultHasher` over the raw diff and stores a `u64` (`cache.rs:27-35`).
- Orphaned review hash uses `DefaultHasher` over group label/files/hunks and returns a `u64` (`review/mod.rs:122-135`).
- Actual per-section cache key uses blake3 over label/files/hunks and returns a 16-char hex string in `ResultDocument::set_groups` (`result.rs:293-310`), then the orchestrator copies it (`orchestrator.rs:142-149`).

**Proposal:** delete the two unused `DefaultHasher` paths. Extract the active group hash from `set_groups`:

```rust
pub fn semantic_group_content_hash(group: &SemanticGroup) -> String {
    let mut h = blake3::Hasher::new();
    h.update(group.label.as_bytes());
    let mut changes = group.changes();
    changes.sort_by(|a, b| a.file.cmp(&b.file));
    for c in &changes {
        h.update(c.file.as_bytes());
        for &hunk in &c.hunks {
            h.update(&hunk.to_le_bytes());
        }
    }
    h.finalize().to_hex().as_str()[..16].to_string()
}
```

Use it in `ResultDocument::set_groups` and in tests for per-section cache identity.

### 3.4 Skill hash/provenance duplicated

- `collect_skill_files` reads and blake3-hashes the skill file for metadata (`orchestrator.rs:333-350`).
- `skill_hash` reads and blake3-hashes the same path for cache invalidation (`review/mod.rs:237-248`).

**Proposal:** expose one helper that returns `SkillFileInfo` for a `ReviewSource`, and have cache invalidation store/compare the same `hash_blake3` string. Example shape:

```rust
pub fn skill_file_info(source: &ReviewSource) -> Option<SkillFileInfo> { ... }
pub fn skill_hash(source: &ReviewSource) -> Option<String> {
    skill_file_info(source).map(|i| i.hash_blake3)
}
```

If `review` should not depend on `result::SkillFileInfo`, return a local `SkillFingerprint { name, path, hash_blake3 }` and convert in orchestrator.

### 3.5 Section label constants are mostly centralized, but tests/cache comments still hard-code them

- Canonical labels live in `ReviewSection::label()` (`review/mod.rs:21-34`).
- Cache schema comment hard-codes `"WHY"/"WHAT"/"HOW"/"VERDICT"` (`review/mod.rs:200`).
- Cache tests hard-code `"WHY"` (`review/mod.rs:394`, `review/mod.rs:448`).
- API tests hard-code the full array (`crates/semantic-diff-cli/tests/runs_api.rs:141`).

**Proposal:** keep user-facing docs if useful, but derive tests from `ReviewSection::all().map(|s| s.label())` to avoid drift.

## 4. Bugs / correctness

### 4.1 Replay leaves SSE subscribers hanging with no producer

- Server/notifier is started before replay (`main.rs:115-135`).
- Replay returns to Ctrl+C without sending `complete` or any other event (`main.rs:157-177`).
- The frontend loads the result then always subscribes to SSE (`web/src/routes/r/[id]/+page.svelte:99-127`).
- `subscribeToResult` only calls `onComplete` when a `section-updated` event has data `complete` (`web/src/lib/api.ts:36-44`).
- The SSE handler subscribes to the channel and emits keep-alives forever; it does not send an initial snapshot or close when the result is already complete (`server.rs:297-323`).
- Existing e2e smoke expects either a `complete` event or clean close within 3s (`web/tests/e2e/tests/cross-cutting.spec.ts:106-164`), which a replayed run does not satisfy.

**Repro:**

1. Run a diff once to completion with default output.
2. Run the same command again without `--no-cache`/`--no-llm`/`--output`.
3. Open DevTools or run the SSE smoke against `/api/result/<id>/events`; the page can fetch the completed JSON, but the EventSource receives only keep-alives and never a `section-updated: complete` event.

**Fix options:**

- Send `let _ = tx.send("complete".to_string())` before the replay wait in `main.rs:170-177`.
- Better: make `sse_handler` check `result.json`; if `status == complete`, emit one `section-updated` event with `complete` and then optionally close.

### 4.2 Whole-result replay invalidates too little and can serve stale reports forever

- Replay condition checks only flags, file existence, parseability, and `status == complete` (`main.rs:157-181`).
- It does not compare current `env!(CARGO_PKG_VERSION)`/`SCHEMA_VERSION` (or even load config before replay; config is loaded only after replay at `main.rs:183-191`).
- Result metadata contains `tool_version`, `schema_version`, and `skill_files` (`result.rs:174-192`) and the orchestrator populates those (`orchestrator.rs:51-64`, `orchestrator.rs:131-136`).

**Failure scenario:** change `review::llm::build_review_prompt` (`review/llm.rs:135-144`) or the built-in HOW/VERDICT instructions, rebuild, and rerun the same diff/title. Replay returns the old report without entering orchestrator, so neither prompt changes nor skill changes are observed.

### 4.3 Mermaid UTF-8 boundary fix is correct; remaining risky slices are currently bounded by ASCII scans

- The fixed loop uses `md[i..].chars().next()` and increments by `len_utf8()` (`review/mermaid_lint.rs:305-310`), with regression coverage for arrows, em dash, emoji, and CJK (`review/mermaid_lint.rs:406-423`).
- Remaining slices in `mermaid_lint.rs` are:
  - bold stripping around ASCII `**` indices (`review/mermaid_lint.rs:170-185`, `review/mermaid_lint.rs:188-198`),
  - label slicing around ASCII delimiters after byte scanning (`review/mermaid_lint.rs:239-283`),
  - mermaid fence slicing around ASCII fence markers and `str::find` results (`review/mermaid_lint.rs:293-301`).
- These are safe because all slice boundaries are either ASCII delimiter offsets or returned by `str::find`. I did not find another `md[i..i+1]`-style pattern in the touched cache/lint files.

**Related non-touched risk:** `compact_message` slices `&compact[..240]` after checking byte length (`llm_cli.rs:457-463`). If stderr/stdout contains non-ASCII and byte 240 is inside a character, this can panic. It is outside the recent cache/lint files, but it matches the same class of UTF-8 issue.

### 4.4 `catch_unwind(AssertUnwindSafe)` around mermaid lint is defensible; verdict parsing is already non-panicking on malformed input

- The wrapper exists only around HOW mermaid lint (`orchestrator.rs:431-461`) and catches panics from `review::lint_markdown_mermaid` (`orchestrator.rs:438-440`). Given the recently-fixed UTF-8 panic, this is a reasonable isolation boundary around complex post-processing after paid LLM calls.
- `AssertUnwindSafe` is not protecting mutable shared state; the closure captures an owned `String` (`orchestrator.rs:437-440`), so the annotation is acceptable.
- VERDICT parsing is invoked inside `ResultDocument::set_section` (`result.rs:363-367`). It uses `pulldown_cmark` offset ranges and `markdown.get(...).unwrap_or("")` for body slicing (`review/verdict.rs:90-158`), and regex construction unwraps only static literals (`review/verdict.rs:57-76`). I did not find an equivalent user-input panic path there.

### 4.5 Replay with `--output` pointing to the deterministic dir is intentionally skipped

- Replay is skipped whenever `cli.output.is_none()` is false (`main.rs:158-161`).
- Therefore `semantic-diff --output ~/.local/share/semantic-diff/results/<id>` re-runs even if that is exactly the deterministic directory. This matches the comment "custom dir; honor user intent" (`main.rs:147-150`).

**Proposed action:** no code fix needed; add a comment only if users are likely to expect replay when manually passing the same directory.

### 4.6 Concurrent same `(diff,title)` runs do not trip replay on in-progress JSON, but can still race as producers

- `write_atomic` writes a temp file and persists/renames atomically (`result.rs:451-469`), so readers should not observe partial JSON.
- Orchestrator writes `status: running` during initial/group/section updates (`orchestrator.rs:79-82`, `orchestrator.rs:136-140`, `orchestrator.rs:293-296`).
- It sets `status: complete` only immediately before the final write (`orchestrator.rs:304-310`), with analogous early-complete paths for empty/no-LLM results (`orchestrator.rs:66-76`, `orchestrator.rs:84-108`).
- Replay additionally checks `status == complete` (`main.rs:165-170`). Therefore an in-progress run should not trigger replay.

**Remaining race:** if two terminals start before either has written a complete result, both pass the replay check and both run the orchestrator to the same output path (`main.rs:157-181`, `orchestrator.rs:49`). Atomic writes prevent torn JSON, but section updates/final result are last-writer-wins. This is probably acceptable for identical inputs, but it is not serialized.

## 5. Cache invalidation gaps

### 5.1 Per-section cache ignores prompt/tool/schema version

- Cache file stores `content_hash`, `source`, optional `skill_hash`, and section states (`review/mod.rs:185-201`).
- Save path stores only those fields (`review/mod.rs:252-270`).
- Load path validates only the hash, review source path/kind, and skill hash (`review/mod.rs:301-323`).
- Prompt construction lives separately in `build_review_prompt` (`review/llm.rs:135-144`), and there is no prompt-version/tool-version input to the cache key.

**Failure scenario:** change HOW prompt instructions, VERDICT schema, or the deterministic mermaid post-processing behavior. Existing per-section cache entries replay old text because the group content and skill hash did not change.

**Fix:** add a cache metadata field such as:

```rust
pub const REVIEW_PROMPT_VERSION: u32 = 1;

pub struct CachedReview {
    pub cache_schema_version: u32,
    pub tool_version: String,
    pub prompt_version: u32,
    // existing fields...
}
```

Then reject entries when any version mismatches. A single `review_cache_fingerprint(&ReviewSource)` helper can combine prompt version, tool version, source path, and skill hash.

### 5.2 Whole-result replay ignores all provenance/invalidation metadata

- Replay does not inspect `metadata` beyond top-level `status` (`main.rs:163-170`).
- `ResultDocument` has top-level `schema_version` (`result.rs:198`) and metadata has `tool_version`, `schema_version`, and `skill_files` (`result.rs:174-192`).
- Orchestrator populates current metadata before writes (`orchestrator.rs:51-64`, `orchestrator.rs:131-136`).

**Failure scenarios:**

- Binary upgrade changes rendering/schema/post-processing; same diff/title serves old schema indefinitely.
- Skill file changes; same diff/title serves old skill review even though per-section cache would have invalidated at `review/mod.rs:311-317` if the orchestrator ran.
- Built-in prompt changes; same diff/title serves old result.

**Fix:** before replaying, load `ResultDocument` and compare:

- `doc.schema_version == SCHEMA_VERSION`,
- `doc.metadata.tool_version == env!("CARGO_PKG_VERSION")`, or a looser compatible-version policy,
- `doc.metadata.skill_files == current collect_skill_files(detect_review_skill())`,
- optional `prompt_version`/`review_engine_version`.

If any mismatch, fall through to orchestrator and let per-section cache decide what can be reused.

## 6. Other quality concerns

### 6.1 Silent cache write/delete failures make cache behavior hard to debug

- `save_sections_to_disk` returns silently when path resolution, parent lookup, directory creation, serialization, temp write, or rename fails (`review/mod.rs:260-277`).
- `delete_review_from_disk` ignores `remove_file` errors (`review/mod.rs:327-330`).

**Recommendation:** keep best-effort semantics, but log `tracing::warn!` for unexpected failures. Example:

```rust
if let Err(e) = std::fs::create_dir_all(dir) {
    tracing::warn!(path=%dir.display(), "failed to create review cache dir: {e}");
    return;
}
```

Use `debug` for benign `NotFound` on delete and `warn` for permission/IO errors.

### 6.2 Clippy: clone on `Copy` `ParseOptions`

- `lint_mermaid` creates `let opts = ParseOptions::default();` (`review/mermaid_lint.rs:121-122`) and calls `parse_metadata_sync(&s, opts.clone())` (`review/mermaid_lint.rs:123`).
- The user-reported clippy warning says `ParseOptions` is `Copy`; if so, call with `opts` directly and reuse `opts` in the retry (`review/mermaid_lint.rs:139`).

### 6.3 `unwrap`/`expect` on user-controlled paths: no high-risk instance in the touched runtime paths

- Runtime path handling generally propagates errors (`main.rs:54`, `main.rs:113`, `orchestrator.rs:74-82`, `result.rs:445-469`) or falls back (`main.rs:58-62`, `review/mod.rs:211-234`).
- `result_path.parent().unwrap_or(...)` in replay mode is safe fallback logic rather than a panic (`main.rs:58-62`).
- Most `unwrap`/`expect` matches in the touched files are tests (`review/mod.rs:353-394`, `orchestrator.rs:518-521`, `orchestrator_streaming.rs:36-196`) or static header parsing in server responses (`server.rs:286-291`).

### 6.4 Whole-result replay lacks tests

- CLI orchestrator tests cover no-LLM output validity, deterministic IDs, and atomic validity (`crates/semantic-diff-cli/tests/orchestrator_streaming.rs:34-207`).
- Core tests cover `ResultDocument::write_atomic` and round-trips (`result.rs:580-595`, `crates/semantic-diff-core/tests/result_roundtrip.rs:84-123`).
- Per-section cache has unit tests in `review/mod.rs:364-455`.
- I found no Rust or web test that exercises the `main.rs` replay branch (`main.rs:143-181`) or asserts replay SSE behavior.

**Recommendation:** extract replay eligibility into a testable helper, e.g. `fn should_replay_result(path, current_fingerprint) -> ReplayDecision`, then unit test complete/running/malformed/stale-skill/stale-version cases. Add one server/SSE test for already-complete replay sending `complete`.

## 7. Recommended action plan

1. **Fix replay invalidation first.** Extract a replay eligibility helper from `main.rs:143-181`; require `status == complete` plus current `schema_version`, compatible `tool_version`, current skill hashes, and a new prompt/review-engine version.
2. **Fix replay SSE behavior.** Either send `complete` before waiting in `main.rs:170-177`, or make `sse_handler` emit/close immediately when `result.json` is already complete (`server.rs:297-323`). Add a regression test matching the replay path.
3. **Centralize active hashes.** Add shared `result_id(diff,title)` and `semantic_group_content_hash(group)` helpers; replace duplicates in `result.rs:248-253`, `main.rs:103-109`, `server.rs:688-695`, and `result.rs:293-310`.
4. **Unify skill fingerprinting.** Replace both `collect_skill_files` (`orchestrator.rs:333-350`) and `skill_hash` (`review/mod.rs:237-248`) with one helper so metadata and cache invalidation compare the same bytes/path representation.
5. **Add per-section cache versioning.** Include `cache_schema_version`, `prompt_version`, and tool/schema compatibility fields in `CachedReview` (`review/mod.rs:190-202`) and validate them in `load_sections_from_disk` (`review/mod.rs:283-323`).
6. **Delete old dead cache code.** Remove `cache.rs` and `pub mod cache` (`lib.rs:4`) if no external public API is promised. Then delete `ReviewCache`, `GroupReview`, `group_content_hash`, and possibly legacy `SectionState` (`review/mod.rs:36-123`, `result.rs:46-55`).
7. **Improve observability of best-effort cache operations.** Add `tracing::warn!`/`debug!` in `save_sections_to_disk` and `delete_review_from_disk` failure branches (`review/mod.rs:260-277`, `review/mod.rs:327-330`).
8. **Clean up small lint/UTF-8 issues.** Remove `opts.clone()` in `mermaid_lint.rs:123`; consider fixing `llm_cli.rs:457-463` with a char-boundary truncation helper.
9. **Add tests.** Cover replay eligibility, stale version/skill invalidation, replay SSE completion, and the shared hash helper consistency between CLI/server/core.
