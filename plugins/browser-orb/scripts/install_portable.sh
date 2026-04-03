#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
VENV_DIR="${REPO_ROOT}/.venv"

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required" >&2
  exit 1
fi

python3 -m venv "${VENV_DIR}"

mkdir -p "${REPO_ROOT}/data/screenshots"

if command -v agent-browser >/dev/null 2>&1; then
  echo "Browser-Orb detected agent-browser at $(command -v agent-browser)"
else
  cat >&2 <<'EOF'
Browser-Orb installed its Python environment, but agent-browser was not found in PATH.

Install agent-browser separately, for example:
  npm install -g agent-browser && agent-browser install
  brew install agent-browser && agent-browser install
  cargo install agent-browser && agent-browser install
EOF
fi

echo "Browser-Orb portable environment installed at ${VENV_DIR}"
