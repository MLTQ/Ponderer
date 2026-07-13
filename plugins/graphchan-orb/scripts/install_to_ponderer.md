# install_to_ponderer.sh

## Purpose
Installs a development checkout into another Ponderer workspace by symlinking the bundle and preparing its portable environment.

## Components

### `install_to_ponderer.sh`
- **Does**: Validates target root, creates `plugins/`, safely refreshes the `graphchan-orb` symlink, and invokes `install_portable.sh`.
- **Interacts with**: Ponderer plugin discovery and `scripts/install_portable.sh`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Plugin discovery | Target contains `plugins/graphchan-orb/plugin.toml` | Changing install name |
| Operators | Existing real directories are never overwritten | Removing the non-symlink guard |

## Notes
- Copying the entire plugin directory is preferable for a standalone portable distribution.
