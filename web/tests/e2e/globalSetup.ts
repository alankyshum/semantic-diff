import { execSync, spawn, ChildProcess } from 'node:child_process';
import { existsSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const E2E_DIR = __dirname;
const REPO_ROOT = path.resolve(E2E_DIR, '../../..');
const WEB_DIR = path.join(REPO_ROOT, 'web');
const RUNTIME_FILE = path.join(E2E_DIR, '.runtime.json');
const BINARY = path.join(REPO_ROOT, 'target/release/semantic-diff');

function log(msg: string) {
  // eslint-disable-next-line no-console
  console.log(`[globalSetup] ${msg}`);
}

async function pollResults(baseURL: string, timeoutMs = 60_000): Promise<string> {
  const deadline = Date.now() + timeoutMs;
  let lastErr: unknown = null;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${baseURL}/api/results`);
      if (res.ok) {
        const body = (await res.json()) as Array<{ id: string }>;
        if (Array.isArray(body) && body.length > 0) return body[0].id;
      }
    } catch (e) {
      lastErr = e;
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(
    `Timed out polling ${baseURL}/api/results for non-empty array. Last error: ${String(lastErr)}`,
  );
}

export default async function globalSetup() {
  // F10: README pre-flight
  log('Running scripts/check-readme.sh');
  try {
    execSync('bash scripts/check-readme.sh', { cwd: REPO_ROOT, stdio: 'inherit' });
  } catch (e) {
    throw new Error(`scripts/check-readme.sh failed (F10 pre-flight): ${String(e)}`);
  }

  // Build web assets if missing or forced
  const buildIndex = path.join(WEB_DIR, 'build/index.html');
  const shouldBuildWeb = !existsSync(buildIndex) || process.env.E2E_BUILD === '1';
  if (shouldBuildWeb) {
    if (!existsSync(path.join(WEB_DIR, 'node_modules'))) {
      log('Running npm install in web/');
      execSync('npm install', { cwd: WEB_DIR, stdio: 'inherit' });
    }
    log('Building web assets');
    execSync('npm run build', { cwd: WEB_DIR, stdio: 'inherit' });
  } else {
    log('web/build exists; skipping rebuild (set E2E_BUILD=1 to force)');
  }

  // cargo build (incremental)
  log('cargo build --release');
  execSync('cargo build --release', { cwd: REPO_ROOT, stdio: 'inherit' });

  if (!existsSync(BINARY)) {
    throw new Error(`Binary not found after build: ${BINARY}`);
  }

  // Spawn server.
  // NOTE: Server's `results_dir = parent(output)` and `/api/result/:id` looks
  // for `<results_dir>/<id>/result.json`. The id is computed from a blake3 hash
  // of the diff at runtime, so we cannot pre-name the dir. Easiest correct
  // solution: omit `--output` entirely and let the binary use its default
  // location (`~/Library/Application Support/semantic-diff/results/<id>/`),
  // where the dir basename matches the id by construction.
  log(`Spawning binary (using default output dir)`);
  const child: ChildProcess = spawn(
    BINARY,
    [
      '--diff',
      'tests/fixtures/real-world.patch',
      '--no-llm',
      '--no-open',
      '--port',
      '0',
    ],
    { cwd: REPO_ROOT, stdio: ['ignore', 'pipe', 'pipe'] },
  );

  const baseURL = await new Promise<string>((resolve, reject) => {
    let stderrBuf = '';
    let stdoutBuf = '';
    const re = /running at (http:\/\/127\.0\.0\.1:\d+)/;
    const timeout = setTimeout(() => {
      reject(
        new Error(
          `Timed out waiting for server URL line.\nstderr:\n${stderrBuf}\nstdout:\n${stdoutBuf}`,
        ),
      );
    }, 30_000);
    child.stderr?.on('data', (d) => {
      const s = d.toString();
      stderrBuf += s;
      process.stderr.write(s);
      const m = re.exec(stderrBuf);
      if (m) {
        clearTimeout(timeout);
        resolve(m[1]);
      }
    });
    child.stdout?.on('data', (d) => {
      const s = d.toString();
      stdoutBuf += s;
      const m = re.exec(stdoutBuf);
      if (m) {
        clearTimeout(timeout);
        resolve(m[1]);
      }
    });
    child.on('exit', (code) =>
      reject(new Error(`Binary exited (code=${code}) before URL line.\nstderr:\n${stderrBuf}`)),
    );
  });

  log(`Server up at ${baseURL}; polling /api/results`);
  const resultId = await pollResults(baseURL);
  log(`First result id: ${resultId}`);

  const runtime = { baseURL, resultId, pid: child.pid };
  writeFileSync(RUNTIME_FILE, JSON.stringify(runtime, null, 2));

  process.env.BASE_URL = baseURL;
  process.env.RESULT_ID = resultId;

  // Detach so child survives outside this process scope; teardown will kill via pid.
  child.unref();
  child.stdout?.unref?.();
  child.stderr?.unref?.();
}
