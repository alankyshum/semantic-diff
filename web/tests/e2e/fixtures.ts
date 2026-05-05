import { test as base, expect } from '@playwright/test';
import { spawn, ChildProcess } from 'node:child_process';
import { mkdtempSync, readFileSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

const REPO_ROOT = path.resolve(__dirname, '../../..');
const BINARY = path.join(REPO_ROOT, 'target/release/semantic-diff');

export interface E2EFixtures {
  baseURL: string;
  resultId: string;
}

export const test = base.extend<E2EFixtures>({
  baseURL: async ({}, use) => {
    const v = process.env.BASE_URL;
    if (!v) throw new Error('BASE_URL not set; globalSetup must run first');
    await use(v);
  },
  resultId: async ({}, use) => {
    const v = process.env.RESULT_ID;
    if (!v) throw new Error('RESULT_ID not set; globalSetup must run first');
    await use(v);
  },
});

export { expect };

export interface ReplayServer {
  baseURL: string;
  resultId: string;
  kill: () => Promise<void>;
}

/**
 * Spawn an isolated semantic-diff server in --result replay mode.
 * Stages the fixture into a fresh `<tmpdir>/<id>/result.json` layout because
 * the server resolves `/api/result/:id` as
 * `parent(parent(--result_path))/<id>/result.json` — so the fixture must live
 * inside a directory named exactly `<id>`.
 *
 * Returns its baseURL, the staged result id, and a kill() helper. Used by
 * F13 / F6-tokens suites.
 */
export async function replayServer(fixturePath: string): Promise<ReplayServer> {
  const abs = path.isAbsolute(fixturePath) ? fixturePath : path.join(REPO_ROOT, fixturePath);
  const doc = JSON.parse(readFileSync(abs, 'utf8')) as { id?: string };
  const id = doc.id;
  if (!id || !/^[0-9a-f]{8}$/.test(id)) {
    throw new Error(`Fixture ${abs} has invalid id field: ${JSON.stringify(id)}`);
  }
  const stageRoot = mkdtempSync(path.join(tmpdir(), 'sd-replay-'));
  const stagedDir = path.join(stageRoot, id);
  mkdirSync(stagedDir, { recursive: true });
  const stagedPath = path.join(stagedDir, 'result.json');
  copyFileSync(abs, stagedPath);

  const child: ChildProcess = spawn(
    BINARY,
    ['--result', stagedPath, '--no-open', '--port', '0'],
    { cwd: REPO_ROOT, stdio: ['ignore', 'pipe', 'pipe'] },
  );

  const baseURL = await new Promise<string>((resolve, reject) => {
    let stderrBuf = '';
    let stdoutBuf = '';
    const re = /(?:running at|Serving result at) (http:\/\/127\.0\.0\.1:\d+)/;
    const timeout = setTimeout(() => {
      reject(new Error(`replayServer timeout. stderr:\n${stderrBuf}\nstdout:\n${stdoutBuf}`));
    }, 15_000);
    child.stderr?.on('data', (d) => {
      stderrBuf += d.toString();
      const m = re.exec(stderrBuf);
      if (m) {
        clearTimeout(timeout);
        resolve(m[1]);
      }
    });
    child.stdout?.on('data', (d) => {
      stdoutBuf += d.toString();
      const m = re.exec(stdoutBuf);
      if (m) {
        clearTimeout(timeout);
        resolve(m[1]);
      }
    });
    child.on('exit', (code) =>
      reject(new Error(`replayServer exited (code=${code}) before URL.\nstderr:\n${stderrBuf}`)),
    );
  });

  const kill = async () => {
    if (!child.pid) return;
    try {
      process.kill(child.pid, 'SIGTERM');
    } catch {
      /* ignore */
    }
    const deadline = Date.now() + 5_000;
    while (Date.now() < deadline) {
      try {
        process.kill(child.pid, 0);
      } catch {
        return;
      }
      await new Promise((r) => setTimeout(r, 100));
    }
    try {
      process.kill(child.pid, 'SIGKILL');
    } catch {
      /* ignore */
    }
  };

  return { baseURL, resultId: id, kill };
}
