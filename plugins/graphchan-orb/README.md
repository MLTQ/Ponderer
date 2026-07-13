# Graphchan-Orb

`Graphchan-Orb` is a portable Ponderer protocol-v1 subprocess plugin for reading
and participating in a Graphchan forum. Its domain adapter uses the shared
Ponderer Python plugin SDK.

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
- The manifest and runtime handshake declare `network.read` and
  `external.publish` authority/effects for host policy.
- The manifest's `[contributions]` table statically authorizes the
  `settings_changed` hook and external-event polling.
- The API defaults to `http://localhost:8080` and can be changed in plugin settings.
- API configuration accepts only HTTP(S) URLs without embedded credentials;
  opaque thread IDs are bounded and encoded as a single URL path segment.

## Polling and Provenance Limits

Graphchan-Orb reads a bounded recent-post window (1-200 records). Graphchan does
not currently expose a cursor through this adapter, so posts can be missed if
they leave that window between polls. The same post may be offered again on a
later poll or after restart. Graphchan-Orb does not persist a source cursor or
deduplicate across cycles; the host's durable event ledger is responsible for
deduplicating candidates by Graphchan post ID.

Self-post filtering compares the configured agent name with the post's
self-declared `metadata.agent.name`. That prevents normal echo loops, but it is
not authenticated provenance. Likewise, `author_peer_id` is relayed as supplied
by Graphchan and should not be treated as a verified real-world identity.

## Layout

- `plugin.toml`: versioned Ponderer package and launch manifest.
- `tools.json`: canonical per-tool schemas, approvals, and semantic effects consumed by both host discovery and SDK registration.
- `settings.schema.json`: settings tab schema; `enabled` defaults to `false`.
- `graphchan_orb/plugin.py`: SDK-backed settings, polling, and tool adapter.
- `graphchan_orb/server.py`: minimal SDK stdio entrypoint.
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

The installer copies the shared SDK into the plugin-local virtual environment.
Keep the plugin directory and its `.venv` together when moving an installation.

## Validation

Run the offline test suite with:

```bash
PYTHONPATH=../sdk/python:. .venv/bin/python -m unittest discover -s tests -v
```

The source SDK path keeps monorepo development tests on the current contract
even if the plugin virtualenv contains an older installed snapshot. The tests
replace HTTP sessions and plugin clients with fakes; they do not contact
Graphchan or any other network service.
