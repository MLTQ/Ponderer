# settings.schema.json

## Purpose
Exposes Vizier enablement and an optional local executable override in Settings.

## Contracts
`enabled` defaults to true for installed packages. `vz_path` defaults to blank
for discovery; an explicit invalid override returns an error, never silently
selects a different executable. Neither setting changes remote SSH PATH lookup.
