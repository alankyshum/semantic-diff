# Rust Code Review

Review Rust code changes for correctness, safety, and idiomatic patterns.

## Steps

1. Get the diff to review:
   ```bash
   git diff origin/main...HEAD
   ```
   If origin/main doesn't exist (new repo), use:
   ```bash
   git diff --cached HEAD
   ```
   If that's empty, diff all commits:
   ```bash
   git log --oneline --all
   ```

2. Run `cargo clippy` for lint checks:
   ```bash
   cd /Users/kshum/Documents/gitproj/semantic-diff && cargo clippy 2>&1
   ```

3. Run `cargo build` to verify compilation:
   ```bash
   cd /Users/kshum/Documents/gitproj/semantic-diff && cargo build 2>&1
   ```

4. Review the diff for:
   - Unsafe code without justification
   - Unwrap/expect on fallible operations in non-test code
   - Missing error handling
   - Unused dependencies
   - Dead code that should be cleaned up

5. After review completes, write the state file to allow push:
   ```bash
   cd /Users/kshum/Documents/gitproj/semantic-diff && git rev-parse HEAD > "$(git rev-parse --git-dir)/.pre-push-reviewed"
   ```

6. Report findings as a short summary.
