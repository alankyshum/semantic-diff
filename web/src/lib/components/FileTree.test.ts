import { describe, it, expect } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import FileTree from './FileTree.svelte';
import type { FileEntry } from '$lib/types';

function fe(path: string, opts: Partial<FileEntry> = {}): FileEntry {
  return {
    path,
    add_lines: 0,
    del_lines: 0,
    group_ids: [],
    max_severity: null,
    ...opts,
  };
}

describe('FileTree', () => {
  it('renders empty-state when files is empty', () => {
    const { getByText } = render(FileTree, { props: { files: [] } });
    expect(getByText('No files.')).toBeTruthy();
  });

  it('renders flat file list with no directory wrappers', () => {
    const files = [
      fe('a.rs', { add_lines: 1 }),
      fe('b.rs', { add_lines: 2 }),
      fe('c.rs', { add_lines: 3 }),
    ];
    const { container } = render(FileTree, { props: { files } });
    const fileBtns = container.querySelectorAll('button.row.file');
    expect(fileBtns.length).toBe(3);
    const dirBtns = container.querySelectorAll('button.row.dir');
    expect(dirBtns.length).toBe(0);
  });

  it('groups nested paths into directory nodes that toggle on click', async () => {
    const files = [
      fe('src/foo.rs'),
      fe('src/bar.rs'),
      fe('tests/baz.rs'),
    ];
    const { container } = render(FileTree, { props: { files } });
    const dirBtns = container.querySelectorAll('button.row.dir');
    expect(dirBtns.length).toBe(2);

    // Both dirs are expanded by default — 3 file rows visible.
    expect(container.querySelectorAll('button.row.file').length).toBe(3);

    // Click `src/` → collapse it. Now only the tests/baz.rs file row remains.
    const srcBtn = Array.from(dirBtns).find(b =>
      b.querySelector('.name')?.textContent === 'src'
    ) as HTMLButtonElement;
    expect(srcBtn).toBeTruthy();
    await fireEvent.click(srcBtn);
    expect(container.querySelectorAll('button.row.file').length).toBe(1);

    // Click again → expand back to 3.
    await fireEvent.click(srcBtn);
    expect(container.querySelectorAll('button.row.file').length).toBe(3);
  });

  it('collapses single-child directory chains (a/b/c/file → "a/b/c" / file)', () => {
    const files = [fe('a/b/c/file.rs')];
    const { container } = render(FileTree, { props: { files } });
    const dirBtns = container.querySelectorAll('button.row.dir');
    expect(dirBtns.length).toBe(1);
    const dirName = dirBtns[0].querySelector('.name')?.textContent;
    expect(dirName).toBe('a/b/c');
    const fileBtns = container.querySelectorAll('button.row.file');
    expect(fileBtns.length).toBe(1);
    expect(fileBtns[0].querySelector('.name')?.textContent).toBe('file.rs');
  });

  it('emits a `select` event with the file path when a file row is clicked', async () => {
    const files = [fe('a.rs'), fe('b.rs')];
    const picked: string[] = [];
    const { container } = render(FileTree, {
      props: { files, selectedFile: null },
      events: { select: (e: CustomEvent<string>) => picked.push(e.detail) },
    });
    const fileBtns = container.querySelectorAll('button.row.file');
    await fireEvent.click(fileBtns[1]);
    expect(picked).toEqual(['b.rs']);
    // After click the row should also reflect the selection class.
    expect((fileBtns[1] as HTMLElement).classList.contains('selected')).toBe(true);
  });

  it('renders a SeverityBadge when a file has max_severity', () => {
    const files = [fe('x.rs', { max_severity: 'high' })];
    const { container } = render(FileTree, { props: { files } });
    // SeverityBadge renders a `.badge.severity-<key>` span.
    const badge = container.querySelector('.badge.severity-high');
    expect(badge).toBeTruthy();
  });

  it('rolls up directory severity to the most-severe descendant', () => {
    const files = [
      fe('pkg/mid.rs', { max_severity: 'medium' }),
      fe('pkg/crit.rs', { max_severity: 'critical' }),
    ];
    const { container } = render(FileTree, { props: { files } });
    const dirBtn = container.querySelector('button.row.dir') as HTMLButtonElement;
    expect(dirBtn).toBeTruthy();
    // The directory's rollup badge is rendered inline inside the dir button.
    const dirBadge = dirBtn.querySelector('.badge') as HTMLElement;
    expect(dirBadge).toBeTruthy();
    expect(dirBadge.classList.contains('severity-critical')).toBe(true);
  });
});
