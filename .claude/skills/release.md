# Release semantic-diff

Publish a new version of semantic-diff with consistent versioning across all distribution channels: Cargo.toml, crates.io, GitHub release (with tag), and Homebrew tap.

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

### 2. Version consistency check (MANDATORY)

Before making any changes, audit all version sources and report mismatches:

```bash
echo "=== Cargo.toml ===" && grep '^version' Cargo.toml
echo "=== Git tags (local) ===" && git tag -l 'v*' | sort -V | tail -5
echo "=== Git tags (remote) ===" && git ls-remote --tags origin 'refs/tags/v*' | awk '{print $2}' | sed 's|refs/tags/||' | sort -V | tail -5
echo "=== GitHub releases ===" && gh release list --limit 5
echo "=== crates.io ===" && cargo search semantic-diff --limit 1 2>/dev/null || echo "(unable to check)"
```

**All of these must align for the target version before the release is considered complete:**
- `Cargo.toml` version matches `<VERSION>`
- Git tag `v<VERSION>` exists locally and on remote
- GitHub release `v<VERSION>` exists with curated notes
- crates.io has `<VERSION>` published
- CI workflow triggered (builds binaries + updates Homebrew tap)

If previous releases have mismatches (e.g., GitHub release exists but no git tag, or crates.io is behind), **report them to the user** and offer to fix before proceeding with the new release.

### 3. Update Cargo.toml version

Edit the `version = "..."` line in `Cargo.toml` to the new version. Skip if already correct.

### 4. Add changelog entry to README (MANDATORY — must be in the release commit)

**This step MUST happen before the commit in Step 7.** The changelog is part of the release commit, not a follow-up.

1. Find the previous release tag:
```bash
git tag -l 'v*' | sort -V | tail -1
```

2. Get all changes since the last tag:
```bash
git log --oneline <previous_tag>..HEAD
```

3. Read the `## Changelog` section in `README.md` and add a new `### v<VERSION>` entry **above** the previous version.

4. Write human-readable bullet points grouped by theme (features, fixes, perf). Format:
```markdown
### v<VERSION>

- **Feature name** — One-sentence description of what it does and why.
- **Fix** — What was broken and how it's fixed.
```

5. Also update the config example block in README if any defaults or options changed.

**The README.md changes must be staged along with Cargo.toml in Step 7's commit.**

### 5. Verify the build

```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && cargo build --release 2>&1
```

### 6. Publish to crates.io

```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && cargo publish 2>&1
```

If this fails with an email verification error, tell the user to verify at https://crates.io/settings/profile.

### 7. Commit, tag, and push

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

Commit the version bump, changelog, and any config doc updates together in a single release commit:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && git add Cargo.toml Cargo.lock README.md && git commit -m "chore: release v<VERSION> with changelog"
```

**Verify README.md is included** — if `git diff --cached README.md` shows no changes, Step 4 was skipped. Go back and add the changelog entry before committing.

Push to main:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && git push origin main
```

### 8. Create GitHub release with curated changelog

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

### 9. Post-release verification (MANDATORY)

After GitHub release is created, verify all channels are consistent:

```bash
echo "=== Verify tag ===" && git fetch --tags && git tag -l "v<VERSION>"
echo "=== Verify GitHub release ===" && gh release view "v<VERSION>" --json tagName,name --jq '.tagName + " " + .name'
echo "=== Verify crates.io ===" && cargo search semantic-diff --limit 1
echo "=== Verify CI triggered ===" && gh run list --limit 1
```

If any channel is missing or mismatched, fix it before proceeding.

### 10. Monitor CI

The release workflow (`.github/workflows/release.yml`) auto-triggers on tag push (created by `gh release create`). It will:
1. Build `aarch64-apple-darwin` and `x86_64-apple-darwin` binaries
2. Upload them to the GitHub release
3. Auto-update the Homebrew tap formula at `alankyshum/homebrew-tap`

Watch the workflow:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && gh run list --limit 1
```

### 11. Report

Print summary with links and verification status:
- Cargo.toml: `<VERSION>` ✓
- crates.io: https://crates.io/crates/semantic-diff ✓/✗
- GitHub release: https://github.com/alankyshum/semantic-diff/releases/tag/v<VERSION> ✓/✗
- Git tag: `v<VERSION>` ✓/✗
- CI workflow: ✓/✗ (triggered / completed)
- Homebrew: `brew install alankyshum/tap/semantic-diff` (updated by CI)

## Prerequisites

- `cargo login` authenticated with crates.io
- `gh` CLI authenticated as `alankyshum`
- Remote URL set to HTTPS (`https://github.com/alankyshum/semantic-diff.git`)
- `HOMEBREW_TAP_TOKEN` secret set in `alankyshum/semantic-diff` repo
