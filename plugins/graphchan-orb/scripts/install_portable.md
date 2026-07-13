# install_portable.sh

## Purpose
Creates a plugin-local Python virtual environment and installs the shared
Ponderer SDK, Graphchan-Orb, and its HTTP dependency.

## Components

### `install_portable.sh`
- **Does**: Verifies Python and the SDK source, creates `.venv` only when absent,
  force-refreshes a local SDK copy, and performs an editable install of the
  plugin bundle.
- **Interacts with**: `plugins/sdk/python`, `pyproject.toml`, and
  `scripts/run_plugin.sh`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `run_plugin.sh` | `.venv/bin/python` exists and imports `graphchan_orb` | Changing venv path or skipping install |
| Operators | Re-running is safe and updates the same local environment | Destructive directory behavior |
| Portable runtime | SDK is copied into the venv rather than imported from a checkout path | Installing the SDK editable |

## Notes
- Initial installation may contact the configured Python package index to install `requests`.
- `PONDERER_PLUGIN_SDK` may override the default sibling path
  `plugins/sdk/python` for standalone development layouts.
- The SDK refresh uses `--force-reinstall` because a local SDK under development
  can change without a package-version bump.
