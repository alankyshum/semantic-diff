# Release semantic-diff

Publish a new version of semantic-diff to crates.io, create a GitHub release with a curated changelog, and trigger the CI workflow that builds dual-arch macOS binaries and auto-updates the Homebrew tap.

## Usage

```
/release [version]
```

- `version` — optional, e.g. `0.4.0`. If omitted, prompt the user.

## Steps

### 1. Determine version

If no version argument provided, check current version and ask user:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && grep '^version' Cargo.toml
```

### 2. Update Cargo.toml version

Edit the `version = "..."` line in `Cargo.toml` to the new version.

### 3. Add changelog entry to README

Read the changelog section in `README.md`. Add a new `### v<VERSION>` entry above the previous version with bullet points summarizing the changes. Use `git log --oneline <previous_tag>..HEAD` to see what changed.

### 4. Verify the build

```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && cargo build --release 2>&1
```

### 5. Publish to crates.io

```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && cargo publish 2>&1
```

If this fails with an email verification error, tell the user to verify at https://crates.io/settings/profile.

### 6. Commit, tag, and push

Ensure the active `gh` account is `alankyshum`:
```bash
gh auth status
```

If the active account is not `alankyshum`, switch:
```bash
gh auth switch --user alankyshum
```

Ensure the remote uses HTTPS (so `gh` handles auth):
```bash
git remote set-url origin https://github.com/alankyshum/semantic-diff.git
```

Commit the version bump and changelog:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && git add Cargo.toml Cargo.lock README.md && git commit -m "chore: release v<VERSION>"
```

Push to main:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && git push origin main
```

### 7. Create GitHub release with curated changelog

Write release notes following this format — do NOT use `--generate-notes`:

```bash
gh release create v<VERSION> --title "v<VERSION>" --notes "$(cat <<'EOF'
## What's Changed

### <Feature Category 1>
- Bullet point summarizing change

### <Feature Category 2>
- Bullet point summarizing change

### Fixes
- Bullet point for any fixes

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

Group changes by theme (e.g. "Adaptive Theme", "Docs & Community", "Fixes") rather than listing raw commits. Use `git log --oneline <previous_tag>..v<VERSION>` to see all changes. Write human-readable descriptions, not commit messages.

### 8. Monitor CI

The release workflow (`.github/workflows/release.yml`) auto-triggers on tag push (created by `gh release create`). It will:
1. Build `aarch64-apple-darwin` and `x86_64-apple-darwin` binaries
2. Upload them to the GitHub release
3. Auto-update the Homebrew tap formula at `alankyshum/homebrew-tap`

Watch the workflow:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && gh run list --limit 1
```

### 9. Report

Print summary with links:
- crates.io: https://crates.io/crates/semantic-diff
- GitHub release: https://github.com/alankyshum/semantic-diff/releases/tag/v<VERSION>
- Homebrew: `brew install alankyshum/tap/semantic-diff`

## Prerequisites

- `cargo login` authenticated with crates.io
- `gh` CLI authenticated as `alankyshum`
- Remote URL set to HTTPS (`https://github.com/alankyshum/semantic-diff.git`)
- `HOMEBREW_TAP_TOKEN` secret set in `alankyshum/semantic-diff` repo
