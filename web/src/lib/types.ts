// Types mirroring result.json schema

export interface SourceInfo {
  kind: 'git_args' | 'diff_file' | 'stdin' | 'pr_url';
  value: string;
}

export interface DiffLine {
  line_type: 'Added' | 'Removed' | 'Context';
  content: string;
}

export interface Hunk {
  header: string;
  source_start: number;
  target_start: number;
  lines: DiffLine[];
}

export interface DiffFile {
  source_file: string;
  target_file: string;
  is_rename: boolean;
  is_untracked: boolean;
  hunks: Hunk[];
  added_count: number;
  removed_count: number;
}

export interface DiffSummary {
  raw: string;
  files: DiffFile[];
  binary_files: string[];
}

export interface GroupChange {
  file: string;
  hunks: number[];
}

export interface Group {
  id: string;
  label: string;
  description: string;
  changes: GroupChange[];
  content_hash: string;
  unified_diff?: string;
}

export interface SectionEntry {
  state: 'loading' | 'ready' | 'error' | 'skipped';
  content?: string;
}

export interface ReviewSourceEntry {
  kind: 'builtin' | 'skill';
  name?: string;
  path?: string;
}

/**
 * Canonical severity values. Matches the on-wire JSON emitted by the Rust
 * backend (`Severity` enum in `crates/semantic-diff-core/src/review/verdict.rs`)
 * which uses `#[serde(rename_all = "lowercase")]`. Frontend code MUST treat
 * `severity` strings as lowercase end-to-end — no PascalCase variants exist.
 */
export type Severity = 'critical' | 'high' | 'medium' | 'low' | 'nit' | 'info';

export interface FileAnchor {
  file: string;
  line?: number | null;
}

export interface Issue {
  id: string;
  severity: Severity;
  title: string;
  body_md: string;
  files: string[];
  anchors: FileAnchor[];
}

export interface GroupReview {
  source: ReviewSourceEntry;
  sections: Record<string, SectionEntry>;
  verdict_issues?: Issue[];
}

export interface RepoInfo {
  name?: string;
  root_path?: string;
  remote_url?: string;
  head_sha?: string;
  branch?: string;
}

export interface LlmInfo {
  provider: string;
  model?: string;
  cli_path?: string;
  cli_version?: string;
}

export interface TokenUsage {
  input_tokens?: number;
  output_tokens?: number;
  cost_usd?: number;
}

export interface SkillFileInfo {
  name: string;
  path: string;
  hash_blake3: string;
}

export interface PerSectionTiming {
  group_id: string;
  section: string;
  duration_ms: number;
  cache_hit: boolean;
}

export interface RunMetadata {
  tool_version: string;
  schema_version: number;
  started_at: string;
  completed_at?: string | null;
  cli_argv: string[];
  working_dir: string;
  llm?: LlmInfo;
  timings: PerSectionTiming[];
  total_duration_ms?: number | null;
  skill_files: SkillFileInfo[];
  tokens?: TokenUsage;
}

export interface ResultDocument {
  schema_version: number;
  id: string;
  title: string;
  created_at: string;
  source: SourceInfo;
  diff: DiffSummary;
  groups: Group[];
  reviews: Record<string, GroupReview>;
  status: 'running' | 'complete' | 'failed';
  repo?: RepoInfo;
  metadata?: RunMetadata;
  file_index?: FileEntry[];
}

/** Per-file rollup emitted by the backend (F12). Wire format is lowercase
 * (see {@link Severity}); `max_severity` is a regular `Severity` value. */
export interface FileEntry {
  path: string;
  add_lines: number;
  del_lines: number;
  group_ids: string[];
  max_severity?: Severity | null;
}

export interface ResultSummary {
  id: string;
  title: string;
  created_at: string;
  status: string;
  repo_name?: string;
}

// ---- F5: Settings / config (mirrors crates/semantic-diff-core/src/config.rs RawConfig) ----

export type AiCli = 'claude' | 'copilot';
export type LlmProviderName = 'claude' | 'copilot' | 'cursor';

export interface CliConfig {
  model: string | null;
}

export interface RawConfig {
  'preferred-ai-cli': AiCli | null;
  'llm-providers': LlmProviderName[] | null;
  claude: CliConfig;
  copilot: CliConfig;
  cursor: CliConfig;
}

export interface ConfigEnvelope {
  path: string | null;
  raw: RawConfig;
  exists: boolean;
  /** Present when the on-disk config file exists but failed to parse.
   * Carries the parse error message so the UI can warn before clobbering. */
  parse_error?: string;
}

/** Status of a `--version` probe. `null`/absent when the binary itself was
 * not found. Mirrors `BinaryProbe::version_status` in the Rust backend. */
export type VersionStatus = 'ok' | 'timeout' | 'error';

export interface BinaryProbe {
  name: string;
  found: boolean;
  path: string | null;
  version: string | null;
  version_status?: VersionStatus;
}

export interface ProviderProbe {
  name: string;
  binaries: BinaryProbe[];
}

export interface ProbeReport {
  providers: ProviderProbe[];
}

/** Default empty RawConfig — used by "Reset to defaults". */
export function defaultRawConfig(): RawConfig {
  return {
    'preferred-ai-cli': null,
    'llm-providers': null,
    claude: { model: null },
    copilot: { model: null },
    cursor: { model: null },
  };
}

// ---- F5 (frontend spec) aliases — keep API parity with the page-level spec. ----
// These alias the canonical types above so both naming schemes round-trip.

/** Alias of {@link LlmProviderName}. */
export type LlmProvider = LlmProviderName;

/** Alias of {@link ConfigEnvelope} — what `GET /api/config` returns. */
export type ConfigPayload = ConfigEnvelope;
