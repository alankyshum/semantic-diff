# Release semantic-diff

Publish a new version of semantic-diff to crates.io and trigger the GitHub release workflow that builds dual-arch macOS binaries and auto-updates the Homebrew tap.

## Usage

```
/release [version]
```

- `version` — optional, e.g. `0.2.0`. If omitted, prompt the user.

## Steps

### 1. Determine version

If no version argument provided, check current version and ask user:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && grep '^version' Cargo.toml
```

### 2. Update Cargo.toml version

Edit the `version = "..."` line in `Cargo.toml` to the new version.

### 3. Verify the build

```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && cargo build --release 2>&1
```

### 4. Publish to crates.io

```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && cargo publish 2>&1
```

If this fails with an email verification error, tell the user to verify at https://crates.io/settings/profile.

### 5. Commit, tag, and push

Switch to the correct GitHub account first:
```bash
gh auth switch --user alankyshum
```

Commit the version bump:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && git add Cargo.toml Cargo.lock && git commit -m "chore: release v<VERSION>"
```

Push to main, then create and push the tag:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && git push origin main && git tag v<VERSION> && git push origin v<VERSION>
```

### 6. Create GitHub release

```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && gh release create v<VERSION> --title "v<VERSION>" --generate-notes
```

### 7. Monitor CI

The release workflow (`.github/workflows/release.yml`) auto-triggers on tag push. It will:
1. Build `aarch64-apple-darwin` and `x86_64-apple-darwin` binaries
2. Upload them to the GitHub release
3. Auto-update the Homebrew tap formula at `alankyshum/homebrew-tap`

Watch the workflow:
```bash
cd /Users/kshum/Documents/gitproj/semantic-diff && gh run list --limit 1
```

### 8. Switch back to default account

```bash
gh auth switch --user kshum_LinkedIn
```

### 9. Report

Print summary with links:
- crates.io: https://crates.io/crates/semantic-diff
- GitHub release: https://github.com/alankyshum/semantic-diff/releases/tag/v<VERSION>
- Homebrew: `brew install alankyshum/tap/semantic-diff`

## Prerequisites

- `cargo login` authenticated with crates.io
- `gh` CLI authenticated as `alankyshum`
- `HOMEBREW_TAP_TOKEN` secret set in `alankyshum/semantic-diff` repo
