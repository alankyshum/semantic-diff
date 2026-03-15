# Roadmap: Semantic Diff TUI

## Milestones

- ✅ **Milestone 1: MVP** -- Phases 1-3 (shipped 2026-03-15, releases v0.1.0–v0.2.3)
- ✅ **Milestone 2: Security & Demo Readiness** -- Phases 4-6 (shipped 2026-03-15, release v0.3.0)

## Phases

<details>
<summary>✅ Milestone 1: MVP (Phases 1-3) -- SHIPPED 2026-03-15</summary>

- [x] Phase 1: Diff Viewer (3/3 plans) -- completed 2026-03-13
- [x] Phase 2: Hook Integration (2/2 plans) -- completed 2026-03-13
- [x] Phase 3: Semantic Grouping (2/2 plans) -- completed 2026-03-15

Full details: `milestones/v1.0-ROADMAP.md`

</details>

### ✅ Milestone 2: Security & Demo Readiness (Shipped)

**Milestone Goal:** Audit all security surfaces, fix all vulnerabilities, and E2E test every claimed feature for YC demo reliability.

- [x] **Phase 4: Red Team -- Security & Dependency Audit** - Identify all vulnerabilities across command execution, signal handling, LLM output trust, path traversal, and dependencies (completed 2026-03-15)
- [x] **Phase 5: Purple Team -- Security Hardening** - Fix all vulnerabilities identified in Phase 4 with defensive hardening across all attack surfaces (completed 2026-03-15)
- [x] **Phase 6: Blue Team -- E2E Demo Testing** - Verify every feature works end-to-end under real-world conditions including edge cases (completed 2026-03-15)

## Phase Details

### Phase 4: Red Team -- Security & Dependency Audit
**Goal**: Complete audit of all attack surfaces producing a documented inventory of vulnerabilities -- no code changes
**Depends on**: Phase 3 (MVP shipped)
**Requirements**: CMD-03, DEP-01, DEP-02
**Success Criteria** (what must be TRUE):
  1. Every `std::process::Command` and `tokio::process::Command` call in the codebase is catalogued with its argument-passing method and risk level
  2. `cargo audit` runs clean or all known vulnerabilities are documented with severity and remediation path
  3. `cargo deny` check passes or all license/duplicate issues are documented
  4. A written audit report covers all four attack surfaces (command injection, signal/PID handling, LLM output trust, path traversal) with specific findings per surface
**Plans:** 2 plans
Plans:
- [ ] 04-01-PLAN.md -- Run cargo audit and cargo deny, capture dependency audit results
- [ ] 04-02-PLAN.md -- Compile comprehensive AUDIT-REPORT.md from all findings

### Phase 5: Purple Team -- Security Hardening
**Goal**: Every identified vulnerability is fixed with defensive code changes across command execution, signal handling, LLM output parsing, and file path validation
**Depends on**: Phase 4
**Requirements**: CMD-01, CMD-02, SIG-01, SIG-02, SIG-03, LLM-01, LLM-02, LLM-03, LLM-04, PATH-01, PATH-02, PATH-03
**Success Criteria** (what must be TRUE):
  1. All shell commands use `Command::new()` with explicit args arrays and LLM prompts are piped via stdin -- no shell interpolation exists anywhere in the codebase
  2. PID file lives in a secure directory with restricted permissions, uses atomic write, and validates process ownership before trusting
  3. All LLM JSON deserialization is bounded by size limits, all string fields are length-validated, and path traversal in LLM responses is rejected
  4. All file paths from git diff output are validated against the repository root and symlinks are resolved before processing
  5. Config file path construction uses safe joins that cannot be tricked by malicious input
**Plans:** 4 plans
Plans:
- [ ] 05-01-PLAN.md -- Harden PID file and log file (secure directory, atomic write, ownership validation)
- [ ] 05-02-PLAN.md -- Add path traversal validation to diff parser, config, and cache paths
- [ ] 05-03-PLAN.md -- Pipe LLM prompts via stdin instead of CLI arguments
- [ ] 05-04-PLAN.md -- Bound LLM response size, validate deserialization, fix UTF-8 truncation

### Phase 6: Blue Team -- E2E Demo Testing
**Goal**: Every feature is verified working end-to-end with automated integration tests, including all edge cases critical for demo reliability
**Depends on**: Phase 5
**Requirements**: TEST-01, TEST-02, TEST-03, TEST-04, TEST-05, TEST-06, TEST-07, TEST-08
**Success Criteria** (what must be TRUE):
  1. An integration test launches the TUI with a known diff and verifies syntax-highlighted output renders correctly with line numbers and word-level diff
  2. An integration test sends SIGUSR1 and verifies the diff view updates without crashing or losing scroll state
  3. An integration test with a mock LLM response verifies semantic grouping appears in the sidebar with correct file assignments
  4. Edge case tests pass for: empty repo (graceful message), large diff (no OOM), binary files (placeholder), clauded unavailable (degradation), malformed LLM JSON (error handled)
**Plans:** 3/3 plans complete
Plans:
- [ ] 06-01-PLAN.md -- Diff rendering, empty repo, and binary file integration tests (TEST-01, TEST-04, TEST-06)
- [ ] 06-02-PLAN.md -- SIGUSR1 signal handling and large diff stress tests (TEST-02, TEST-05)
- [ ] 06-03-PLAN.md -- LLM grouping, unavailability, and malformed JSON tests (TEST-03, TEST-07, TEST-08)

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Diff Viewer | MVP | 3/3 | Complete | 2026-03-13 |
| 2. Hook Integration | MVP | 2/2 | Complete | 2026-03-13 |
| 3. Semantic Grouping | MVP | 2/2 | Complete | 2026-03-15 |
| 4. Red Team Audit | Security | 2/2 | Complete | 2026-03-15 |
| 5. Purple Team Hardening | Security | 4/4 | Complete | 2026-03-15 |
| 6. Blue Team E2E Testing | Security | 3/3 | Complete | 2026-03-15 |
