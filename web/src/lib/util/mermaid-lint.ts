/**
 * Mermaid linter & auto-fixer for LLM-generated diagrams.
 *
 * Common LLM mistakes:
 *  1. Unquoted node labels containing special chars: ( ) [ ] { } | # &
 *  2. Missing diagram direction (e.g. `flowchart` without `TD`/`LR`)
 *  3. Smart quotes " " ' ' instead of ASCII " '
 *  4. HTML entities &amp; &lt; in labels
 *  5. Empty node labels
 *  6. Trailing semicolons (valid but sometimes cause issues)
 *  7. Extra backtick fences around the body
 *  8. Markdown bold **text** inside labels
 */

export interface LintResult {
  /** The (possibly fixed) diagram source. */
  fixed: string;
  /** Human-readable warnings. */
  warnings: string[];
  /** True if any fixes were applied. */
  modified: boolean;
}

/** Lint and auto-fix a single mermaid diagram body (no fences). */
export function lintMermaid(source: string): LintResult {
  const warnings: string[] = [];
  let s = source;
  let modified = false;

  // --- Strip stray backtick fences the LLM sometimes wraps around the body ---
  const fenceRe = /^```(?:mermaid)?\s*\n([\s\S]*?)\n```\s*$/;
  const fenceMatch = s.match(fenceRe);
  if (fenceMatch) {
    s = fenceMatch[1];
    warnings.push('Stripped extra backtick fence wrapper');
    modified = true;
  }

  // --- Smart quotes → ASCII ---
  const smartBefore = s;
  s = s.replace(/[\u201C\u201D\u201E\u201F\u2033]/g, '"')
       .replace(/[\u2018\u2019\u201A\u201B\u2032]/g, "'");
  if (s !== smartBefore) {
    warnings.push('Replaced smart quotes with ASCII quotes');
    modified = true;
  }

  // --- HTML entities ---
  const entityBefore = s;
  s = s.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"');
  if (s !== entityBefore) {
    warnings.push('Decoded HTML entities');
    modified = true;
  }

  // --- Missing direction on flowchart ---
  const fcMatch = s.match(/^(\s*flowchart)\s*$/m);
  if (fcMatch) {
    s = s.replace(/^(\s*flowchart)\s*$/m, '$1 TD');
    warnings.push('Added missing direction (TD) to flowchart');
    modified = true;
  }

  // --- Markdown bold inside node labels: **text** → text ---
  const boldBefore = s;
  s = s.replace(/\*\*([^*]+)\*\*/g, '$1');
  if (s !== boldBefore) {
    warnings.push('Stripped markdown bold (**) from labels');
    modified = true;
  }

  // --- Unquoted node labels with special chars ---
  // Match patterns like: A[some (text)] or B(some [text])
  // where the label contains chars that need quoting
  const needsQuote = /[[\](){}|#&<>]/;
  const nodeDefRe = /^(\s*\w+)\[([^\]"]+)\]/gm;
  const nodeDef2Re = /^(\s*\w+)\(([^)"]+)\)/gm;

  function quoteLabels(src: string, re: RegExp, open: string, close: string): string {
    return src.replace(re, (match, prefix: string, label: string) => {
      if (needsQuote.test(label) && !label.startsWith('"')) {
        const escaped = label.replace(/"/g, '#quot;');
        warnings.push(`Quoted label: ${label.trim()}`);
        modified = true;
        return `${prefix}${open}"${escaped}"${close}`;
      }
      return match;
    });
  }

  s = quoteLabels(s, nodeDefRe, '[', ']');
  s = quoteLabels(s, nodeDef2Re, '(', ')');

  // --- Empty lines inside diagram that break parsing ---
  // Remove runs of 3+ blank lines (keep at most 1)
  const blankBefore = s;
  s = s.replace(/\n{3,}/g, '\n\n');
  if (s !== blankBefore) {
    warnings.push('Collapsed excessive blank lines');
    modified = true;
  }

  return { fixed: s, warnings, modified };
}

/**
 * Try to render mermaid source; if it fails, lint+fix and retry.
 * Returns the SVG string or throws the final error.
 */
export async function renderWithAutoFix(
  mermaid: typeof import('mermaid').default,
  source: string,
  id: string,
): Promise<{ svg: string; warnings: string[] }> {
  // First try: render as-is
  try {
    const { svg } = await mermaid.render(id, source);
    return { svg, warnings: [] };
  } catch (firstError) {
    // Second try: lint & fix, then re-render
    const { fixed, warnings, modified } = lintMermaid(source);
    if (!modified) {
      // Nothing to fix, re-throw original error
      throw firstError;
    }
    try {
      const retryId = `${id}-fix`;
      const { svg } = await mermaid.render(retryId, fixed);
      warnings.unshift('Auto-fixed diagram errors');
      return { svg, warnings };
    } catch (secondError) {
      // Both attempts failed — throw the second (post-fix) error
      throw secondError;
    }
  }
}
