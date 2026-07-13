#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
VENV_DIR="${REPO_ROOT}/.venv"
SDK_ROOT="${PONDERER_PLUGIN_SDK:-${REPO_ROOT}/../sdk/python}"

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required" >&2
  exit 1
fi

if [[ ! -f "${SDK_ROOT}/pyproject.toml" ]]; then
  echo "Ponderer Python plugin SDK not found at ${SDK_ROOT}" >&2
  echo "Set PONDERER_PLUGIN_SDK to the SDK's python package directory." >&2
  exit 1
fi

if [[ ! -x "${VENV_DIR}/bin/python" ]]; then
  python3 -m venv "${VENV_DIR}"
fi
"${VENV_DIR}/bin/python" -m pip install --disable-pip-version-check \
  --force-reinstall --no-deps "${SDK_ROOT}"
"${VENV_DIR}/bin/python" -m pip install --disable-pip-version-check -e "${REPO_ROOT}"

echo "Graphchan-Orb portable environment installed at ${VENV_DIR}"
