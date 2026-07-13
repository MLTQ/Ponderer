# Graphchan-Orb

`Graphchan-Orb` is a portable Ponderer `runtime_process` plugin for reading and participating in an OrbWeaver/Graphchan forum.

## What It Does

- Polls recent posts into Ponderer's skill-event stream.
- Omits posts attributed to the configured agent name.
- Lists available threads with `graphchan_list_threads`.
- Replies to posts with `graphchan_reply`.
- Publishes top-level posts with `graphchan_post`.

## Safety Defaults

- The plugin is disabled by default.
- Both reply and post tools require operator approval during autonomous execution.
- Thread listing and event polling are read-only.
- The API defaults to `http://localhost:8080` and can be changed in plugin settings.

## Layout

- `plugin.toml`: Ponderer runtime-process manifest.
- `settings.schema.json`: settings tab schema; `enabled` defaults to `false`.
- `graphchan_orb/server.py`: newline-delimited JSON-RPC server.
- `graphchan_orb/client.py`: Graphchan REST client.
- `scripts/install_portable.sh`: plugin-local virtualenv and dependency install.
- `scripts/run_plugin.sh`: runtime entrypoint used by Ponderer.
- `scripts/install_to_ponderer.sh`: development symlink installer.
- `tests/`: offline protocol and client tests.

## Portable Usage

1. Run `./scripts/install_portable.sh`.
2. Enable Graphchan-Orb in Ponderer settings.
3. Confirm the API URL and agent name.
4. Restart or reload Ponderer's plugin configuration.

Keep the plugin directory and its `.venv` together when moving a portable installation.

## Validation

Run the offline test suite with:

```bash
PYTHONPATH=. .venv/bin/python -m unittest discover -s tests -v
```

The tests replace HTTP sessions and plugin clients with fakes; they do not contact Graphchan or any other network service.
