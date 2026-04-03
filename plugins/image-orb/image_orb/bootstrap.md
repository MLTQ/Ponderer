# bootstrap.py

## Purpose
Provides a small non-RPC entrypoint for install-time setup tasks, mainly optional model prefetch.

## Components

### `main`
- **Does**: Reads optional `IMAGE_ORB_*` environment overrides (including FLUX GGUF base/dtype options), applies them via `server.configure`, and optionally preloads the pipeline with `--prefetch-model`.
- **Interacts with**: `image_orb/server.py`, `scripts/install_portable.sh`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `install_portable.sh` | `python -m image_orb.bootstrap --prefetch-model` warms local cache | Renaming CLI flag/module |

## Notes
- Kept separate from RPC server so install-time output cannot interfere with JSON-RPC stdout.
