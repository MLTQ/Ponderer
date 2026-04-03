# install_portable.sh

## Purpose
Creates a self-contained Python virtual environment inside the plugin repo and installs `Browser-Orb` there.

## Components

### `install_portable.sh`
- **Does**: Creates `.venv`, creates the plugin-local screenshot folder, and warns if the external `agent-browser` binary is missing from PATH.
- **Interacts with**: `pyproject.toml`, `scripts/run_plugin.sh`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `scripts/run_plugin.sh` | `.venv/bin/python` exists after install | Changing venv location |
| Operators | Script prepares a portable plugin checkout in place | Removing editable install or data-dir creation |

## Notes
- Browser-Orb intentionally does not auto-install `agent-browser`; that dependency may come from npm, Homebrew, or Cargo depending on the operator's preference.
- Browser-Orb has no third-party Python dependencies, so the installer intentionally skips package installation and runs the server directly from the checkout via `PYTHONPATH`.
