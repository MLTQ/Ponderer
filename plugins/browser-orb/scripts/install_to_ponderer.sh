#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PONDERER_ROOT="${1:-}"

if [[ -z "${PONDERER_ROOT}" ]]; then
  echo "usage: $0 /path/to/Ponderer" >&2
  exit 1
fi

INSTALL_ROOT="${PONDERER_ROOT}/plugins"
TARGET="${INSTALL_ROOT}/browser-orb"

mkdir -p "${INSTALL_ROOT}"

if [[ -e "${TARGET}" && ! -L "${TARGET}" ]]; then
  echo "refusing to replace non-symlink target: ${TARGET}" >&2
  exit 1
fi

ln -sfn "${REPO_ROOT}" "${TARGET}"
echo "Installing Browser-Orb runtime dependencies into ${TARGET}..."
"${TARGET}/scripts/install_portable.sh"
echo "Installed Browser-Orb into ${TARGET}"
