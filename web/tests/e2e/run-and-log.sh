#!/usr/bin/env bash
# Run the full E2E suite and tee output to a stable log path the orchestrator can read.
set -uo pipefail

cd "$(dirname "$0")"

LOG=/tmp/e2e-run.log
echo "=== $(date -u +%FT%TZ) E2E run start ===" | tee "$LOG"

# Install if missing
if [ ! -x node_modules/.bin/playwright ]; then
  echo "[run-and-log] Installing npm deps..." | tee -a "$LOG"
  npm install 2>&1 | tee -a "$LOG"
  npx playwright install chromium 2>&1 | tee -a "$LOG"
fi

# Run suite (globalSetup handles cargo + web build + server spawn)
npx playwright test --reporter=list 2>&1 | tee -a "$LOG"
RC=${PIPESTATUS[0]}

echo "=== $(date -u +%FT%TZ) E2E run end (rc=$RC) ===" | tee -a "$LOG"
exit $RC
