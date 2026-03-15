# Requirements: Semantic Diff TUI — v1.1 Security & Demo Readiness

**Defined:** 2026-03-15
**Core Value:** Show Claude's code changes in real-time with AI-powered semantic grouping, so the user always knows what's being changed and can mentally track the work without leaving the terminal.

## v1.1 Requirements

Requirements for security hardening and YC demo readiness.

### Command Execution Safety

- [x] **CMD-01**: All shell commands use `Command::new()` with explicit args array, never shell interpolation
- [x] **CMD-02**: LLM prompt content passed via stdin pipe instead of CLI argument to avoid arg-length limits and shell metacharacter risks
- [x] **CMD-03**: Audit all `std::process::Command` and `tokio::process::Command` calls for argument safety

### Signal & PID Safety

- [x] **SIG-01**: PID file uses secure directory with restricted permissions (e.g., `$XDG_RUNTIME_DIR`) instead of world-writable `/tmp/`
- [x] **SIG-02**: PID file creation uses atomic write (write-to-temp + rename) to prevent TOCTOU races
- [x] **SIG-03**: PID file validates ownership before trusting (check if PID belongs to semantic-diff process)

### LLM Output Trust

- [x] **LLM-01**: Bound serde deserialization of LLM JSON with size limits to prevent memory exhaustion
- [x] **LLM-02**: Validate all string fields from LLM output (label, description) have bounded lengths
- [x] **LLM-03**: Validate file paths in LLM grouping response don't contain path traversal (`../`)
- [x] **LLM-04**: Cache file reads validate JSON structure before full deserialization

### File Path Safety

- [x] **PATH-01**: Validate file paths from git diff output don't escape repository root (no `../` traversal)
- [x] **PATH-02**: Resolve symlinks before processing diff files to prevent symlink-following attacks
- [x] **PATH-03**: Config file path (`~/.config/semantic-diff.json`) uses safe path construction

### E2E Demo Testing

- [x] **TEST-01**: Integration test for live diff rendering (launch with known diff, verify TUI output)
- [ ] **TEST-02**: Integration test for SIGUSR1 refresh (send signal, verify diff updates)
- [ ] **TEST-03**: Integration test for semantic grouping (mock LLM response, verify sidebar)
- [x] **TEST-04**: Edge case test: empty repo (no changes), verify graceful "No changes detected"
- [ ] **TEST-05**: Edge case test: large diff (>1000 files), verify no crash or OOM
- [x] **TEST-06**: Edge case test: binary files in diff, verify placeholder rendering
- [ ] **TEST-07**: Edge case test: clauded unavailable, verify graceful degradation
- [ ] **TEST-08**: Edge case test: malformed LLM JSON response, verify error handling

### Dependency Audit

- [x] **DEP-01**: Run `cargo audit` and fix all known vulnerabilities in dependencies
- [x] **DEP-02**: Run `cargo deny` check for license compliance and duplicate dependencies

## Future Requirements

### Enhanced Security

- **ESEC-01**: Fuzzing harness for diff parser (cargo-fuzz)
- **ESEC-02**: Property-based testing for all parsers (proptest)
- **ESEC-03**: Sandboxed LLM process execution (seccomp/pledge)

## Out of Scope

| Feature | Reason |
|---------|--------|
| Network security / TLS | Tool is local-only, no network connections |
| Authentication / authorization | Single-user terminal tool, no multi-user access |
| Encrypted config | Config contains only model preferences, no secrets |
| Penetration testing of Claude CLI itself | Out of scope — we only audit our invocation of it |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| CMD-01 | Phase 5 | Complete |
| CMD-02 | Phase 5 | Complete |
| CMD-03 | Phase 4 | Complete |
| SIG-01 | Phase 5 | Complete |
| SIG-02 | Phase 5 | Complete |
| SIG-03 | Phase 5 | Complete |
| LLM-01 | Phase 5 | Complete |
| LLM-02 | Phase 5 | Complete |
| LLM-03 | Phase 5 | Complete |
| LLM-04 | Phase 5 | Complete |
| PATH-01 | Phase 5 | Complete |
| PATH-02 | Phase 5 | Complete |
| PATH-03 | Phase 5 | Complete |
| TEST-01 | Phase 6 | Complete |
| TEST-02 | Phase 6 | Pending |
| TEST-03 | Phase 6 | Pending |
| TEST-04 | Phase 6 | Complete |
| TEST-05 | Phase 6 | Pending |
| TEST-06 | Phase 6 | Complete |
| TEST-07 | Phase 6 | Pending |
| TEST-08 | Phase 6 | Pending |
| DEP-01 | Phase 4 | Complete |
| DEP-02 | Phase 4 | Complete |

**Coverage:**
- v1.1 requirements: 23 total
- Mapped to phases: 23
- Unmapped: 0

---
*Requirements defined: 2026-03-15*
*Last updated: 2026-03-15 after roadmap creation*
