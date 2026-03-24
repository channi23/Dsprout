#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PID_DIR="$ROOT_DIR/.dsprout/pids"

stop_pid_file() {
  local name="$1"
  local pid_file="$PID_DIR/$name.pid"

  if [[ -f "$pid_file" ]]; then
    local pid
    pid="$(cat "$pid_file")"
    if kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
      echo "stopped $name (pid $pid)"
    fi
    rm -f "$pid_file"
  fi
}

stop_pid_file "frontend"
stop_pid_file "agent"
stop_pid_file "satellite"

for port in 3000 7070 7081 5901; do
  if lsof -t -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    lsof -t -iTCP:"$port" -sTCP:LISTEN | xargs kill >/dev/null 2>&1 || true
  fi
done

echo "DSprout stopped."
