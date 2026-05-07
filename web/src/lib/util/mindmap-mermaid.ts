// Convert mermaid `mindmap` block syntax → nested markdown that markmap-lib understands.
//
// Ported from the share--markdown skill so semantic-diff renders mermaid mindmaps
// using interactive markmap (pan/zoom/expand) instead of a static mermaid SVG.
//
// Mermaid mindmap uses indentation for hierarchy and various node-shape brackets:
//   mindmap
//     root((Centre))
//       Branch 1
//         [Square node]
//         (Round node)
//
//   - First non-empty non-`mindmap` line is the root node.
//   - Each indent step (relative width) = one level deeper.
//   - Shape brackets — ((text)), (text), [text], {{text}}, ))text((, )text( — are stripped.
//   - <br/> and <br> tags become spaces.
//   - id-prefix syntax `id[text]` keeps the text only.

const SHAPE_PATTERNS: { open: string; close: string }[] = [
  { open: '((',  close: '))'  },   // circle
  { open: '))',  close: '(('  },   // cloud
  { open: '{{',  close: '}}'  },   // hexagon
  { open: ')',   close: '('   },   // bang
  { open: '(',   close: ')'   },   // round
  { open: '[',   close: ']'   },   // square
  { open: '{',   close: '}'   },   // cloud (alt)
];

/** Strip mermaid's node-shape syntax + optional id prefix. */
function stripShape(raw: string): string {
  let s = raw.trim();
  const idMatch = s.match(/^[A-Za-z0-9_-]+(?=[[({])/);
  if (idMatch) s = s.slice(idMatch[0].length);

  for (const { open, close } of SHAPE_PATTERNS) {
    if (s.startsWith(open) && s.endsWith(close) && s.length >= open.length + close.length) {
      s = s.slice(open.length, s.length - close.length).trim();
      break;
    }
  }
  s = s.replace(/<br\s*\/?>/gi, ' ').replace(/\s+/g, ' ').trim();
  return s;
}

/** Compute leading-whitespace width of a line (tab = 4). */
function leadingWidth(line: string): number {
  let w = 0;
  for (const ch of line) {
    if (ch === ' ') w++;
    else if (ch === '\t') w += 4;
    else break;
  }
  return w;
}

/** Parse a mermaid `mindmap` block into the nested-markdown form markmap-lib expects.
 *  Returns null if the source isn't a recognizable mindmap. */
export function mermaidMindmapToMarkdown(source: string): string | null {
  const rawLines = source.split(/\r?\n/);
  let i = 0;
  while (i < rawLines.length && rawLines[i].trim() === '') i++;
  if (i >= rawLines.length) return null;
  if (!/^\s*mindmap\b/i.test(rawLines[i])) return null;
  i++;

  type Node = { depth: number; text: string };
  const nodes: Node[] = [];
  for (; i < rawLines.length; i++) {
    const line = rawLines[i];
    if (!line.trim()) continue;
    if (/^\s*%%/.test(line)) continue;
    nodes.push({ depth: leadingWidth(line), text: stripShape(line) });
  }
  if (nodes.length === 0) return null;

  const sortedIndents = Array.from(new Set(nodes.map(n => n.depth))).sort((a, b) => a - b);
  const indentMap = new Map(sortedIndents.map((w, idx) => [w, idx]));
  const normalised = nodes.map(n => ({ level: indentMap.get(n.depth)!, text: n.text }));

  const out: string[] = [];
  let rootEmitted = false;
  for (const { level, text } of normalised) {
    if (level === 0 && !rootEmitted) {
      out.push(`# ${text}`);
      rootEmitted = true;
    } else if (level === 0) {
      out.push(`## ${text}`);
    } else {
      out.push(`${'  '.repeat(level - 1)}- ${text}`);
    }
  }
  return out.join('\n');
}

/** Quick check: does this mermaid block start with the `mindmap` keyword? */
export function isMermaidMindmap(source: string): boolean {
  return /^\s*(?:%%[^\n]*\n\s*)*mindmap\b/i.test(source);
}
