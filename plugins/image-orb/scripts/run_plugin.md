# run_plugin.sh

## Purpose
Launches the `Image-Orb` JSON-RPC server from the plugin-local virtual environment.

## Components

### `run_plugin.sh`
- **Does**: Verifies `.venv` exists, enables unbuffered Python I/O, disables user-site package injection, and executes `python -m image_orb.server`.
- **Interacts with**: `plugin.toml`, `image_orb/server.py`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `plugin.toml` | This script is the plugin entrypoint command | Renaming or moving script |
| Ponderer runtime host | Server uses stdio and emits one JSON response line per request | Switching transport |

