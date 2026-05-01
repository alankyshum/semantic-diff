#!/usr/bin/env bash
set -euo pipefail
# Fail if README.md references removed concepts.
# Note: '.git/semantic-diff-cache.json' is STILL VALID and is intentionally NOT denied.
DENY=(ratatui SIGUSR1 'copilot --yolo' 'Press p to toggle' 'Help overlay')
FAIL=0
for term in "${DENY[@]}"; do
  if grep -q -E -- "$term" README.md; then
    echo "README.md still references stale term: $term" >&2
    FAIL=1
  fi
done
exit $FAIL
