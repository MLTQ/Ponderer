#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
VENV_DIR="${REPO_ROOT}/.venv"
DEFAULT_TORCH_CUDA_INDEX_URL="https://download.pytorch.org/whl/cu124"

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required" >&2
  exit 1
fi

python3 -m venv "${VENV_DIR}"
source "${VENV_DIR}/bin/activate"

python -m pip install --upgrade pip setuptools wheel
if [[ -n "${IMAGE_ORB_TORCH_INDEX_URL:-}" ]]; then
  python -m pip install --upgrade torch --index-url "${IMAGE_ORB_TORCH_INDEX_URL}"
elif command -v nvidia-smi >/dev/null 2>&1; then
  python -m pip install --upgrade torch --index-url "${DEFAULT_TORCH_CUDA_INDEX_URL}"
else
  python -m pip install --upgrade torch
fi
python -m pip install -e "${REPO_ROOT}"

python - <<'PY'
import torch

mps_backend = getattr(getattr(torch, "backends", None), "mps", None)
mps_available = bool(mps_backend is not None and mps_backend.is_available())
print(
    "Image-Orb torch backend:",
    {
        "version": torch.__version__,
        "cuda_available": bool(torch.cuda.is_available()),
        "cuda_version": torch.version.cuda,
        "mps_available": mps_available,
    },
)
PY

mkdir -p \
  "${REPO_ROOT}/data/models" \
  "${REPO_ROOT}/data/output" \
  "${REPO_ROOT}/data/state"

if [[ "${IMAGE_ORB_PREFETCH_MODEL:-0}" == "1" ]]; then
  echo "Prefetching Image-Orb model into local cache..."
  python -m image_orb.bootstrap --prefetch-model
else
  echo "Skipping model prefetch (set IMAGE_ORB_PREFETCH_MODEL=1 to prefetch)."
fi

echo "Image-Orb portable environment installed at ${VENV_DIR}"
