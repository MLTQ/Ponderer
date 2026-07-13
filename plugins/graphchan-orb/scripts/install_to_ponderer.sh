#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PONDERER_ROOT="${1:-}"
SDK_ROOT="${PONDERER_PLUGIN_SDK:-${REPO_ROOT}/../sdk/python}"

if [[ -z "${PONDERER_ROOT}" ]]; then
  echo "usage: $0 /path/to/Ponderer" >&2
  exit 1
fi

if [[ ! -f "${SDK_ROOT}/pyproject.toml" ]]; then
  echo "Ponderer Python plugin SDK not found at ${SDK_ROOT}" >&2
  echo "Set PONDERER_PLUGIN_SDK to the SDK's python package directory." >&2
  exit 1
fi

INSTALL_ROOT="${PONDERER_ROOT}/plugins"
TARGET="${INSTALL_ROOT}/graphchan-orb"

mkdir -p "${INSTALL_ROOT}"

if [[ -e "${TARGET}" && ! -L "${TARGET}" ]]; then
  echo "refusing to replace non-symlink target: ${TARGET}" >&2
  exit 1
fi

ln -sfn "${REPO_ROOT}" "${TARGET}"
echo "Installing Graphchan-Orb runtime dependencies into ${TARGET}..."
PONDERER_PLUGIN_SDK="${SDK_ROOT}" "${TARGET}/scripts/install_portable.sh"
echo "Installed Graphchan-Orb into ${TARGET}"
