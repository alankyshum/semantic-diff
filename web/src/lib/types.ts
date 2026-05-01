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

export interface GroupReview {
  source: ReviewSourceEntry;
  sections: Record<string, SectionEntry>;
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
}

export interface ResultSummary {
  id: string;
  title: string;
  created_at: string;
  status: string;
}
