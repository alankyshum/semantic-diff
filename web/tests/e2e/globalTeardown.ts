import { existsSync, readFileSync, unlinkSync } from 'node:fs';
import path from 'node:path';

const RUNTIME_FILE = path.join(__dirname, '.runtime.json');

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

export default async function globalTeardown() {
  if (!existsSync(RUNTIME_FILE)) return;
  let runtime: { pid?: number } = {};
  try {
    runtime = JSON.parse(readFileSync(RUNTIME_FILE, 'utf8'));
  } catch {
    return;
  }
  const pid = runtime.pid;
  if (typeof pid === 'number' && alive(pid)) {
    try {
      process.kill(pid, 'SIGTERM');
    } catch {
      /* ignore */
    }
    const deadline = Date.now() + 5_000;
    while (Date.now() < deadline && alive(pid)) {
      await new Promise((r) => setTimeout(r, 100));
    }
    if (alive(pid)) {
      try {
        process.kill(pid, 'SIGKILL');
      } catch {
        /* ignore */
      }
    }
  }
  try {
    unlinkSync(RUNTIME_FILE);
  } catch {
    /* ignore */
  }
}
