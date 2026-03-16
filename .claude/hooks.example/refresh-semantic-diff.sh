#!/bin/bash
# PostToolUse hook: refresh semantic-diff or launch it in a cmux split
# Called by Claude Code after Edit/Write tool calls (async, non-blocking)
PIDFILE="/tmp/semantic-diff.pid"
SURFACEFILE="/tmp/semantic-diff-surface.id"

# If semantic-diff is already running, just signal it to refresh
if [ -f "$PIDFILE" ]; then
    PID=$(cat "$PIDFILE")
    if ps -p "$PID" -o comm= 2>/dev/null | grep -q semantic-diff; then
        kill -USR1 "$PID" 2>/dev/null
        exit 0
    fi
    # Stale PID file — remove it
    rm -f "$PIDFILE"
fi

# semantic-diff not running — launch in cmux split if available
if ! command -v cmux >/dev/null 2>&1; then
    exit 0
fi

# Check if we already have a surface from a previous launch
if [ -f "$SURFACEFILE" ]; then
    SURFACE=$(cat "$SURFACEFILE")
    # Verify the surface still exists
    if cmux list-pane-surfaces 2>/dev/null | grep -q "$SURFACE"; then
        # Surface exists but semantic-diff isn't running — relaunch in it
        cmux send --surface "$SURFACE" "cd \"${CLAUDE_PROJECT_DIR:-.}\" && semantic-diff\n" 2>/dev/null
        exit 0
    fi
    # Surface gone — remove stale file
    rm -f "$SURFACEFILE"
fi

# No existing surface — create a new right split
OUTPUT=$(cmux new-split right 2>&1)
SURFACE=$(echo "$OUTPUT" | grep -o 'surface:[0-9]*' | head -1)

if [ -n "$SURFACE" ]; then
    echo "$SURFACE" > "$SURFACEFILE"

    # Note: cmux resize-pane is not yet implemented, so splits default to 50/50.
    # Manually drag the divider to adjust. Track: cmux new-split --size support.

    # Small delay to let the terminal initialize
    sleep 0.3
    cmux send --surface "$SURFACE" "cd \"${CLAUDE_PROJECT_DIR:-.}\" && semantic-diff\n" 2>/dev/null
fi

exit 0
