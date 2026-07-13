# install_portable.sh

## Purpose
Creates a plugin-local Python virtual environment and installs Graphchan-Orb with its HTTP dependency.

## Components

### `install_portable.sh`
- **Does**: Verifies Python, creates `.venv`, and performs an editable install of the plugin bundle.
- **Interacts with**: `pyproject.toml`, `scripts/run_plugin.sh`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `run_plugin.sh` | `.venv/bin/python` exists and imports `graphchan_orb` | Changing venv path or skipping install |
| Operators | Re-running is safe and updates the same local environment | Destructive directory behavior |

## Notes
- Initial installation may contact the configured Python package index to install `requests`.
