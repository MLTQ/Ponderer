#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d -t ponderer-living-loop-XXXXXX)"
MOCK_PORT="${MOCK_OPENAI_PORT:-19091}"
BACKEND_BIND="${PONDERER_BACKEND_BIND:-127.0.0.1:8880}"
BACKEND_URL="http://${BACKEND_BIND}"
WS_URL="ws://${BACKEND_BIND}/v1/ws/events"
BACKEND_LOG="${WORK_DIR}/backend.log"
MOCK_LOG="${WORK_DIR}/mock_llm.log"
WS_EVENTS_LOG="${WORK_DIR}/ws_events.jsonl"
WS_READY_FILE="${WORK_DIR}/ws.ready"
DB_NAME="living_loop_validation.db"
DB_PATH_CANDIDATE="${WORK_DIR}/${DB_NAME}"

cleanup() {
  kill "${WS_PID:-}" >/dev/null 2>&1 || true
  kill "${BACKEND_PID:-}" >/dev/null 2>&1 || true
  kill "${MOCK_PID:-}" >/dev/null 2>&1 || true
  wait "${WS_PID:-}" >/dev/null 2>&1 || true
  wait "${BACKEND_PID:-}" >/dev/null 2>&1 || true
  wait "${MOCK_PID:-}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

cat >"${WORK_DIR}/ponderer_config.toml" <<CFG
graphchan_api_url = ""
llm_api_url = "http://127.0.0.1:${MOCK_PORT}"
llm_model = "mock-model"
username = "Ponderer"
poll_interval_secs = 1
enable_ambient_loop = true
ambient_min_interval_secs = 5
enable_journal = true
journal_min_interval_secs = 60
enable_concerns = true
enable_dream_cycle = true
dream_min_interval_secs = 3600
enable_image_generation = false
enable_self_reflection = false
enable_screen_capture_in_loop = false
enable_camera_capture_tool = false
max_posts_per_hour = 10
database_path = "living_loop_validation.db"
CFG
# Ensure this config is selected over executable-directory defaults.
touch -t 209912312359 "${WORK_DIR}/ponderer_config.toml"

echo "Working directory: ${WORK_DIR}"
echo "Starting mock OpenAI server on :${MOCK_PORT}"
python3 "${REPO_ROOT}/scripts/mock_openai_server.py" --host 127.0.0.1 --port "${MOCK_PORT}" >"${MOCK_LOG}" 2>&1 &
MOCK_PID=$!
sleep 1

echo "Starting standalone backend on ${BACKEND_BIND}"
(
  cd "${WORK_DIR}"
  RUST_LOG=info,ponderer_backend=debug \
  PONDERER_BACKEND_BIND="${BACKEND_BIND}" \
  PONDERER_BACKEND_AUTH_MODE=disabled \
  cargo run -q --manifest-path "${REPO_ROOT}/ponderer_backend/Cargo.toml" --bin ponderer_backend
) >"${BACKEND_LOG}" 2>&1 &
BACKEND_PID=$!

READY=0
for _ in $(seq 1 60); do
  if curl -fsS "${BACKEND_URL}/v1/health" >/dev/null 2>&1; then
    READY=1
    break
  fi
  sleep 0.5
done

if [[ "${READY}" != "1" ]]; then
  echo "Backend did not become healthy in time" >&2
  tail -n 40 "${BACKEND_LOG}" || true
  exit 1
fi

echo "Capturing websocket events"
node -e '
const fs = require("fs");
const url = process.argv[1];
const out = process.argv[2];
const durationMs = Number(process.argv[3]);
const ready = process.argv[4];
const events = [];
const WebSocketCtor = globalThis.WebSocket || (() => {
  try { return require("ws"); } catch { return null; }
})();
if (!WebSocketCtor) {
  fs.writeFileSync(out, "");
  process.exit(3);
}
const ws = new WebSocketCtor(url);
let done = false;
const finish = (code) => {
  if (done) return;
  done = true;
  try { fs.writeFileSync(out, events.join("\n")); } catch {}
  try { ws.close(); } catch {}
  process.exit(code);
};
ws.onopen = () => {
  try { fs.writeFileSync(ready, "ready"); } catch {}
};
ws.onmessage = (evt) => {
  events.push(String(evt.data));
};
ws.onerror = () => finish(2);
ws.onclose = () => finish(0);
setTimeout(() => finish(0), durationMs);
' "${WS_URL}" "${WS_EVENTS_LOG}" "18000" "${WS_READY_FILE}" >"${WORK_DIR}/ws.log" 2>&1 &
WS_PID=$!

WS_READY=0
for _ in $(seq 1 80); do
  if [[ -f "${WS_READY_FILE}" ]]; then
    WS_READY=1
    break
  fi
  sleep 0.25
done

if [[ "${WS_READY}" != "1" ]]; then
  echo "WebSocket client did not reach open state in time" >&2
  echo "WS capture log:" >&2
  cat "${WORK_DIR}/ws.log" >&2 || true
  echo "Backend log tail:" >&2
  tail -n 40 "${BACKEND_LOG}" >&2 || true
  exit 1
fi

echo "Triggering deterministic state-change events"
curl -sS -X POST "${BACKEND_URL}/v1/agent/toggle-pause" >/dev/null
sleep 0.2
curl -sS -X POST "${BACKEND_URL}/v1/agent/toggle-pause" >/dev/null

wait "${WS_PID}" || true

if [[ ! -s "${WS_EVENTS_LOG}" ]]; then
  echo "No websocket events captured" >&2
  echo "WS capture log:" >&2
  cat "${WORK_DIR}/ws.log" >&2 || true
  echo "Backend log tail:" >&2
  tail -n 40 "${BACKEND_LOG}" >&2 || true
  exit 1
fi

if ! python3 - "${WS_EVENTS_LOG}" <<'PY'
import json
import sys

path = sys.argv[1]
event_types = set()
for line in open(path, "r", encoding="utf-8"):
    line = line.strip()
    if not line:
        continue
    try:
        payload = json.loads(line)
    except json.JSONDecodeError:
        continue
    event_type = payload.get("event_type")
    if event_type:
        event_types.add(event_type)

if "state_changed" not in event_types:
    print("Missing required websocket event type: state_changed. Seen:", sorted(event_types))
    sys.exit(1)
sys.exit(0)
PY
then
  echo "Living loop event stream missing required events" >&2
  echo "WS capture log:" >&2
  cat "${WORK_DIR}/ws.log" >&2 || true
  exit 1
fi

DB_PATH="${DB_PATH_CANDIDATE}"
if [[ ! -f "${DB_PATH}" ]]; then
  DB_FROM_LOG="$(sed -n "s/.*Agent memory database initialized: \(.*${DB_NAME}\).*/\1/p" "${BACKEND_LOG}" | tail -n 1)"
  if [[ -n "${DB_FROM_LOG}" && -f "${DB_FROM_LOG}" ]]; then
    DB_PATH="${DB_FROM_LOG}"
  else
    echo "Expected DB file not found. Checked: ${DB_PATH_CANDIDATE}" >&2
    if [[ -n "${DB_FROM_LOG}" ]]; then
      echo "Backend-reported DB path: ${DB_FROM_LOG}" >&2
    fi
    tail -n 40 "${BACKEND_LOG}" >&2 || true
    exit 1
  fi
fi

if ! python3 - "${DB_PATH}" <<'PY'
import sqlite3
import sys

db_path = sys.argv[1]
conn = sqlite3.connect(db_path)
cur = conn.cursor()
cur.execute("SELECT count(*) FROM orientation_snapshots")
orientation_count = int(cur.fetchone()[0])
cur.execute("SELECT count(*) FROM concerns")
concern_count = int(cur.fetchone()[0])
cur.execute("SELECT count(*) FROM journal_entries")
journal_count = int(cur.fetchone()[0])
conn.close()

if orientation_count < 1:
    print("Expected at least one orientation snapshot")
    sys.exit(1)

print(
    f"orientation_snapshots={orientation_count} concerns={concern_count} journal_entries={journal_count}"
)
sys.exit(0)
PY
then
  echo "Living loop DB validation failed" >&2
  exit 1
fi

if ! grep -q "Loaded config from" "${BACKEND_LOG}"; then
  echo "Backend did not report loading temp config file" >&2
  exit 1
fi

if [[ ! -s "${MOCK_LOG}" ]]; then
  echo "Mock LLM log is empty; backend likely did not call mock model" >&2
  exit 1
fi

echo "[ok] living loop acceptance baseline"
echo "Work dir: ${WORK_DIR}"
echo "Backend log tail:"
tail -n 25 "${BACKEND_LOG}" || true
echo "Mock LLM log tail:"
tail -n 10 "${MOCK_LOG}" || true
