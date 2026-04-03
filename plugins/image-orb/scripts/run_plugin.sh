#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PYTHON_BIN="${REPO_ROOT}/.venv/bin/python"

if [[ ! -x "${PYTHON_BIN}" ]]; then
  echo "Image-Orb is not installed. Run ./scripts/install_portable.sh first." >&2
  exit 1
fi

export PYTHONUNBUFFERED=1
export PYTHONNOUSERSITE=1
exec "${PYTHON_BIN}" -m image_orb.server

