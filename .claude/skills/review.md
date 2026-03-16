# Rust Code Review

Review Rust code changes for correctness, safety, idiomatic patterns, regressions, and scope discipline. Automatically split oversized commits.

## Steps

1. Get the diff to review:
   ```bash
   git diff origin/main...HEAD
   ```
   If that's empty, try unstaged changes:
   ```bash
   git diff
   ```
   If origin/main doesn't exist (new repo), use:
   ```bash
   git diff --cached HEAD
   ```

2. Run `cargo clippy` for lint checks:
   ```bash
   cd /Users/kshum/Documents/gitproj/semantic-diff && cargo clippy 2>&1
   ```

3. Run `cargo build` to verify compilation:
   ```bash
   cd /Users/kshum/Documents/gitproj/semantic-diff && cargo build 2>&1
   ```

4. Run `cargo test` to catch regressions:
   ```bash
   cd /Users/kshum/Documents/gitproj/semantic-diff && cargo test 2>&1
   ```

5. **Regression analysis** — Review the diff for changes that may break existing behavior:
   - Changed function signatures (parameters added/removed/retyped)
   - Modified match arms or conditional branches that alter control flow
   - Removed or renamed public items (structs, enums, functions, methods)
   - Changed default values or config parsing logic
   - Modified serialization/deserialization formats
   - Altered event handling or key binding behavior
   Flag each potential regression with the affected area and severity.

6. **Code quality review** — Check for:
   - Unsafe code without justification
   - Unwrap/expect on fallible operations in non-test code
   - Missing error handling
   - Unused dependencies
   - Dead code that should be cleaned up

7. **Scope analysis — Detect out-of-scope changes:**
   Classify every changed file into a logical concern (e.g., "config parsing", "diff engine", "UI rendering", "file tree sidebar", "grouper logic"). Then:
   - Identify the **primary intent** of the changeset (the concern touching the most files/lines)
   - Flag files whose concern does **not** match the primary intent as **out-of-scope**
   - Flag unrelated formatting-only, refactor-only, or drive-by fixes as out-of-scope
   - Report a scope summary table:
     | Concern | Files | In-scope? |
     |---------|-------|-----------|

8. **Auto-split into smaller commits** if the changeset spans multiple concerns:
   - Group changed files by concern from step 7
   - For each concern group, stage only its files and create a focused commit
   - Use clear commit message prefixes: `feat:`, `fix:`, `refactor:`, `chore:`
   - Commit order: foundational changes first (models/types → logic → UI → config)
   - If a single file contains changes for multiple concerns, use `git add -p` to stage only the relevant hunks
   - **Ask the user for confirmation** before creating any commits, showing the proposed split plan:
     ```
     Proposed commit split:
     1. feat(diff): add untracked file support — src/diff/mod.rs, src/diff/untracked.rs, src/diff/parser.rs
     2. refactor(grouper): improve group filtering — src/grouper/mod.rs
     3. feat(ui): enhance file tree with search and icons — src/ui/file_tree.rs, src/ui/diff_view.rs
     4. fix(config): update default settings — src/config.rs, src/main.rs
     ```
   - Only proceed with committing after user approval

9. After review and commits complete, write the state file to allow push:
   ```bash
   cd /Users/kshum/Documents/gitproj/semantic-diff && git rev-parse HEAD > "$(git rev-parse --git-dir)/.pre-push-reviewed"
   ```

10. Report findings as a structured summary:
    - **Build/Lint**: pass/fail
    - **Tests**: pass/fail (number of tests)
    - **Regressions**: list of potential regressions found
    - **Scope**: in-scope vs out-of-scope breakdown
    - **Commits**: split plan (proposed or executed)
    - **Action items**: anything the user should address
