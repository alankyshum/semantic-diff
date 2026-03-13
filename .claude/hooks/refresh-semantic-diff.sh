#!/bin/bash
# PostToolUse hook: refresh semantic-diff or launch it in a cmux split
# Called by Claude Code after Edit/Write tool calls (async, non-blocking)
PIDFILE="/tmp/semantic-diff.pid"

if [ -f "$PIDFILE" ]; then
    PID=$(cat "$PIDFILE")
    # Verify the process is actually semantic-diff (macOS compatible)
    if ps -p "$PID" -o comm= 2>/dev/null | grep -q semantic-diff; then
        kill -USR1 "$PID" 2>/dev/null
        exit 0
    fi
    # Stale PID file — remove it
    rm -f "$PIDFILE"
fi

# semantic-diff not running — launch in cmux split if available
if command -v cmux >/dev/null 2>&1; then
    # Create a right split and send the launch command
    NEW_SURFACE=$(cmux new-split right --json 2>/dev/null | jq -r '.surface // empty')
    if [ -n "$NEW_SURFACE" ]; then
        cmux send --surface "$NEW_SURFACE" "cd \"${CLAUDE_PROJECT_DIR:-.}\" && semantic-diff\n"
    else
        # Fallback: try without --json if cmux version doesn't support it
        cmux new-split right 2>/dev/null
    fi
fi

exit 0
