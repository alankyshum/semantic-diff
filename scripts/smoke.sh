#!/usr/bin/env bash
# scripts/smoke.sh — end-to-end smoke test
# Runs the full pipeline against a fixture diff, verifies key contracts.
# Exit code 0 = pass.

set -euo pipefail

BINARY="${1:-./target/release/semantic-diff}"
FIXTURE="tests/fixtures/real-world.patch"
OUTPUT_DIR="/tmp/sd-smoke-$$"
PORT=8765

cleanup() {
  [ -n "${SERVER:-}" ] && kill "$SERVER" 2>/dev/null || true
  rm -rf "$OUTPUT_DIR"
}
trap cleanup EXIT

if [ ! -f "$FIXTURE" ]; then
  echo "ERROR: fixture not found: $FIXTURE"
  echo "Create a test fixture with: git diff HEAD~3 HEAD > tests/fixtures/real-world.patch"
  exit 1
fi

if [ ! -f "$BINARY" ]; then
  echo "ERROR: binary not found: $BINARY"
  echo "Run: cargo build --release"
  exit 1
fi

mkdir -p "$OUTPUT_DIR"

echo "Starting semantic-diff server..."
"$BINARY" --diff "$FIXTURE" --output "$OUTPUT_DIR" --no-open --port "$PORT" --no-llm &
SERVER=$!

echo "Waiting for server to start..."
for i in $(seq 1 10); do
  if curl -s "http://localhost:$PORT/api/results" > /dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

echo "Checking /api/results..."
curl -fsS "http://localhost:$PORT/api/results" | \
  python3 -c "import json,sys; data=json.load(sys.stdin); assert len(data) > 0, 'expected at least 1 result'"

echo "Getting result ID..."
ID=$(curl -s "http://localhost:$PORT/api/results" | python3 -c "import json,sys; print(json.load(sys.stdin)[0]['id'])")
echo "Result ID: $ID"

echo "Checking /api/result/$ID..."
curl -fsS "http://localhost:$PORT/api/result/$ID" | \
  python3 -c "
import json, sys
doc = json.load(sys.stdin)
assert doc['status'] == 'complete', f'expected complete, got {doc[\"status\"]}'
assert doc['schema_version'] == 1, f'expected schema_version 1'
print(f'OK: status={doc[\"status\"]}, groups={len(doc[\"groups\"])}, id={doc[\"id\"]}')
"

echo "Checking SPA at /..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:$PORT/")
[ "$STATUS" = "200" ] || { echo "ERROR: / returned $STATUS"; exit 1; }

echo "Checking SPA route /r/$ID..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:$PORT/r/$ID")
[ "$STATUS" = "200" ] || { echo "ERROR: /r/$ID returned $STATUS"; exit 1; }

echo ""
echo "✓ All smoke tests passed."
