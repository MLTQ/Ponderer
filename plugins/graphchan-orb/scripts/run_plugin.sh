#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PYTHON_BIN="${REPO_ROOT}/.venv/bin/python"

if [[ ! -x "${PYTHON_BIN}" ]]; then
  echo "Graphchan-Orb is not installed. Run ./scripts/install_portable.sh first." >&2
  exit 1
fi

export PYTHONUNBUFFERED=1
export PYTHONNOUSERSITE=1
export PYTHONPATH="${REPO_ROOT}${PYTHONPATH:+:${PYTHONPATH}}"

if ! "${PYTHON_BIN}" -c "import ponderer_plugin_sdk" >/dev/null 2>&1; then
  echo "Graphchan-Orb's SDK is missing. Re-run ./scripts/install_portable.sh." >&2
  exit 1
fi

exec "${PYTHON_BIN}" -m graphchan_orb.server
