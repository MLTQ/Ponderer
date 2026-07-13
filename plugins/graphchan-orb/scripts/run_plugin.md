# run_plugin.sh

## Purpose
Launches Graphchan-Orb's SDK-owned stdio RPC server from the plugin-local
virtual environment.

## Components

### `run_plugin.sh`
- **Does**: Verifies the venv and installed SDK, enables unbuffered isolated
  Python I/O, exposes the bundle on `PYTHONPATH`, and runs
  `graphchan_orb.server`.
- **Interacts with**: `plugin.toml`, `install_portable.sh`, the shared SDK, and
  `graphchan_orb/server.py`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `plugin.toml` | Script remains at `scripts/run_plugin.sh` | Moving or renaming it |
| Runtime host | Stdout contains only correlated JSON response lines | Printing diagnostics to stdout |

## Notes
- Missing-install diagnostics go to stderr so they cannot corrupt the JSON-RPC stream.
