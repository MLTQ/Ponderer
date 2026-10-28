# Vizier for Ponderer

Makes [Vizier](https://github.com/MLTQ/vizier)'s `vz` CLI available as model tools:

- `vz({"mode":"wake"})`: Local machine, user, resources and environment.
- `vz({"mode":"snapshot"})`: Fresh local desktop/network state.
- `vz_ssh({"host":"my-server","mode":"wake"})`: The same orientation on an
  SSH host. Uses SSH config aliases, keys/agent and existing host verification.

Both default to compact wake observations; `verbose: true` requests raw detail.
Results identify the target and include Vizier's timestamp and schema version.
Guidance reminds the model to orient locally and when first working on an SSH
host. Tools are available in agentic mode; direct chat does not execute tools.

## Install

Requires Python 3.10+ and a native `vz` binary. From the Ponderer repository:

```sh
python3 plugins/vizier-orb/scripts/install_to_ponderer.py target/release/plugins
python3 plugins/vizier-orb/scripts/install_to_ponderer.py target/debug/plugins
```

For another installation, pass its `plugins` directory beside
`ponderer_config.toml`, or the configured `PONDERER_PLUGIN_DIR`. The installer
copies the shared SDK; no pip install or virtual environment is needed. The
running backend discovers the enabled package automatically. Disable it in the
Vizier settings tab if desired. Re-run the installer after adapter updates.

Local executable discovery checks PATH, `~/.local/bin`, `~/.cargo/bin`,
`~/Code/vizier/target/{release,debug}`, and Homebrew locations, including GUI
launches with a minimal PATH. Set **Vizier executable** or `PONDERER_VZ_BIN` to
choose a specific binary. The plugin does not modify your shell configuration.

## SSH hosts

`vz_ssh` runs `vz` on the destination, with `~/.local/bin` and common package
manager paths included. The optional `port` overrides SSH config; omit it to use
the host's configured port. A host must already be reachable noninteractively.
Connection failures are returned as errors, with normal SSH diagnostics.

If Vizier is missing, the result includes an upstream install command for the
model to run using the normal shell tool, then retry. Installation is separate
from observation and follows the existing tool approval policy. Upstream
prebuilt targets are Apple Silicon macOS and x86_64 Linux; other machines need
a native source build. A local Mac executable is never copied to Linux.

`vz watch --diff` remains available through a tracked background shell command
when continuous monitoring is requested; these tools always return one shot.
The public-IP lookup is disabled. Wake can include recent files and shell
history, and desktop collectors depend on platform permissions. These are
best-effort observations and are supplied to the model as data.

## Verification

```sh
python3 -m unittest discover -s plugins/vizier-orb/tests -v
python3 plugins/vizier-orb/scripts/validate_backend.py target/release/ponderer target/release/plugins
```

Tests cover protocol/static-contract agreement, executable discovery, argument
validation, compact/verbose modes, SSH quoting/config, missing installs, timeouts,
bounded output, and a portable installed-package launch without the source SDK.
The backend smoke check uses a temporary config and a loopback HTTP server to
verify the installed package against the actual host. Use a plugin directory
containing only Vizier so unrelated installed plugins do not start during it.
