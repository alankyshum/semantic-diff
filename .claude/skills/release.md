# Release semantic-diff

Publish a new version to crates.io, GitHub Releases, and Homebrew (via CI).

## Usage

```
/release [version]
```

If version is omitted, check current version and ask user.

## Steps

### 1. Determine version

```bash
grep '^version' Cargo.toml
git tag -l 'v*' | sort -V | tail -3
```

### 2. Update Cargo.toml

Edit `version = "..."` to the new version. Run `cargo build` to update Cargo.lock.

### 3. Build and test

```bash
cargo clippy -- -D warnings 2>&1
cargo build --release 2>&1
cargo test 2>&1
```

All must pass before proceeding.

### 4. Publish to crates.io

```bash
cargo publish 2>&1
```

If this fails with email verification, tell user to verify at https://crates.io/settings/profile.

### 5. Commit and push

Ensure `gh` is authenticated as `alankyshum` and remote uses HTTPS:
```bash
gh auth status
git remote set-url origin https://github.com/alankyshum/semantic-diff.git
```

Commit version bump:
```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: release v<VERSION>"
git push origin main
```

### 6. Create GitHub release

Write curated release notes from `git log --oneline <previous_tag>..HEAD`. Group by theme, not raw commits. Do NOT use `--generate-notes`.

```bash
gh release create v<VERSION> --title "v<VERSION>" --notes "$(cat <<'EOF'
## What's Changed

- **Feature** — Description
- **Fix** — Description

## Install / Upgrade

\`\`\`bash
cargo install semantic-diff
# or
brew install alankyshum/tap/semantic-diff
\`\`\`

**Full Changelog**: https://github.com/alankyshum/semantic-diff/compare/v<PREVIOUS>...v<VERSION>
EOF
)"
```

This creates the git tag and triggers the release CI workflow which:
1. Builds `aarch64-apple-darwin` and `x86_64-apple-darwin` binaries
2. Uploads them to the GitHub release
3. Auto-updates the Homebrew tap at `alankyshum/homebrew-tap`

### 7. Verify

```bash
echo "=== Tag ===" && git tag -l "v<VERSION>"
echo "=== GitHub release ===" && gh release view "v<VERSION>" --json tagName,name --jq '.tagName + " " + .name'
echo "=== crates.io ===" && cargo search semantic-diff --limit 1
echo "=== CI ===" && gh run list --limit 1
```

Report summary:
- crates.io: https://crates.io/crates/semantic-diff
- GitHub release: https://github.com/alankyshum/semantic-diff/releases/tag/v<VERSION>
- Homebrew: `brew install alankyshum/tap/semantic-diff` (updated by CI)

## Prerequisites

- `cargo login` authenticated with crates.io
- `gh` CLI authenticated as `alankyshum`
- `HOMEBREW_TAP_TOKEN` secret set in `alankyshum/semantic-diff` repo
