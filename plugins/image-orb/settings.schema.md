# settings.schema.json

## Purpose
Defines Image-Orb plugin settings fields rendered by the host UI and persisted into plugin configuration. The schema controls defaults, user-facing labels/help text, and available tuning knobs.

## Components

### `fields[]`
- **Does**: Declares each configurable setting (`key`, `title`, `kind`, help/default/options) used by runtime `plugin.configure`.
- **Interacts with**: `image_orb/server.py` merged settings and generation/load behavior.

### `device` / `cuda_device_index`
- **Does**: Exposes backend selection for local inference. `device=auto` preserves the default `cuda:0 -> mps -> cpu` preference order, while `device=cuda` plus `cuda_device_index` lets operators pin Image-Orb to a secondary GPU such as `cuda:1`.
- **Interacts with**: `resolve_effective_device` in `image_orb/server.py`.

### `single_file_config_dir` / `single_file_config_repo`
- **Does**: Exposes offline single-file checkpoint bootstrap controls:
  `single_file_config_dir` stores local Diffusers config artifacts and
  `single_file_config_repo` optionally overrides the bootstrap source repo.
- **Interacts with**: `ensure_single_file_config_dir` and `bootstrap_single_file_config_dir` in `image_orb/server.py`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Ponderer settings UI | Stable field keys/kinds/defaults | Renaming keys or changing field kind |
| `server.py` configure flow | Values are scalar/JSON-friendly and parseable | Changing types without parser updates |

## Notes
- `single_file_config_repo` is optional. When blank, server-side family defaults are used.
- The schema is UI metadata only; validation and guardrails still happen in runtime code.
