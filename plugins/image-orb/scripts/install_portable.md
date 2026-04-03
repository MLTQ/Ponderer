# install_portable.sh

## Purpose
Creates a self-contained Python virtual environment inside the plugin repo and installs `Image-Orb` plus its dependencies there.

## Components

### `install_portable.sh`
- **Does**: Creates `.venv`, upgrades packaging tools, installs `torch` first (allowing explicit `IMAGE_ORB_TORCH_INDEX_URL` override or CUDA auto-selection when `nvidia-smi` exists), installs the project in editable mode, prints detected torch backend capabilities (`cuda`/`mps`) for install-time verification, creates portable `data/` directories (models/output/state), and optionally prefetches the configured model when `IMAGE_ORB_PREFETCH_MODEL=1`.
- **Interacts with**: `pyproject.toml`, `image_orb/bootstrap.py`, `scripts/run_plugin.sh`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `scripts/run_plugin.sh` | `.venv/bin/python` exists after install | Changing venv location |
| Operators | Script prepares a portable plugin checkout in place | Removing editable install or data-dir creation |

## Notes
- Prefetch is opt-in because model size and gating vary by selected model family/reference.
- Set `IMAGE_ORB_TORCH_INDEX_URL` to force a specific PyTorch wheel channel when auto-detection is not enough.
