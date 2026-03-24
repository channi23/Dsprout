#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_DIR="$ROOT_DIR/server"
APP_DIR="$ROOT_DIR/app"
STATE_DIR="$ROOT_DIR/.dsprout"
LOG_DIR="$STATE_DIR/logs"
PID_DIR="$STATE_DIR/pids"

mkdir -p "$LOG_DIR" "$PID_DIR"

detect_lan_ip() {
  local ip=""

  ip="$(ipconfig getifaddr en0 2>/dev/null || true)"
  if [[ -z "$ip" ]]; then
    ip="$(ipconfig getifaddr en1 2>/dev/null || true)"
  fi
  if [[ -z "$ip" ]]; then
    ip="$(ifconfig | awk '/inet / { print $2 }' | grep -v '^127\.' | head -n 1 || true)"
  fi

  if [[ -z "$ip" ]]; then
    echo "failed to detect LAN IP" >&2
    exit 1
  fi

  echo "$ip"
}

wait_for_http() {
  local url="$1"
  local label="$2"

  for _ in $(seq 1 40); do
    if curl -sf "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  echo "timed out waiting for $label at $url" >&2
  exit 1
}

start_bg() {
  local name="$1"
  local workdir="$2"
  local logfile="$3"
  local port="$4"
  shift 4

  if lsof -t -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "$name already appears to be running on port $port"
    return 0
  fi

  (
    cd "$workdir"
    nohup "$@" >"$logfile" 2>&1 &
    echo $! >"$PID_DIR/$name.pid"
  )
}

LAN_IP="$(detect_lan_ip)"
SATELLITE_URL="http://$LAN_IP:7070"
LOCAL_AGENT_URL="http://127.0.0.1:7081"
ADVERTISE_MULTIADDR="/ip4/$LAN_IP/tcp/5901"

cat >"$APP_DIR/.env.local" <<EOF
SATELLITE_URL=$SATELLITE_URL
LOCAL_AGENT_URL=$LOCAL_AGENT_URL
EOF

echo "LAN IP: $LAN_IP"
echo "Satellite URL: $SATELLITE_URL"
echo "Local agent URL: $LOCAL_AGENT_URL"

start_bg "satellite" "$SERVER_DIR" "$LOG_DIR/satellite.log" 7070 env DSPROUT_PUBLIC_URL="$SATELLITE_URL" cargo run -p dsprout-satellite
wait_for_http "http://127.0.0.1:7070/workers" "satellite"

start_bg "agent" "$SERVER_DIR" "$LOG_DIR/agent.log" 7081 cargo run -p dsprout-agent
wait_for_http "http://127.0.0.1:7081/status" "agent"

curl -sf -X POST "$LOCAL_AGENT_URL/config" \
  -H 'content-type: application/json' \
  -d "{
    \"listen_multiaddr\": \"/ip4/0.0.0.0/tcp/5901\",
    \"advertise_multiaddr\": \"$ADVERTISE_MULTIADDR\",
    \"restart_if_running\": true
  }" >/dev/null

curl -sf -X POST "$LOCAL_AGENT_URL/start" \
  -H 'content-type: application/json' \
  -d '{}' >/dev/null

start_bg "frontend" "$APP_DIR" "$LOG_DIR/frontend.log" 3000 npm run dev
wait_for_http "http://127.0.0.1:3000" "frontend"

echo
echo "DSprout started."
echo "Frontend:  http://localhost:3000"
echo "Satellite: $SATELLITE_URL"
echo "Agent:     $LOCAL_AGENT_URL"
echo "Worker:    $ADVERTISE_MULTIADDR"
echo
echo "Logs:"
echo "  $LOG_DIR/satellite.log"
echo "  $LOG_DIR/agent.log"
echo "  $LOG_DIR/frontend.log"
