# runner.py

## Purpose
Runs bounded one-shot Vizier commands locally or through SSH. Supports the CLI
already installed by the operator without linking a checkout-specific Rust path.

## Components
- `resolve_vz`: Checks explicit setting/environment override, PATH, user bins,
  `~/Code/vizier` builds and Homebrew locations. Overrides fail explicitly.
- `observation_args`: Restricts modes and flags; uses bare CLI for compact
  snapshots and always skips Vizier's external public-IP lookup.
- `ssh_prefix`: Validates a host and optional port. Uses noninteractive SSH with
  normal config, keys and host verification; never forwards arbitrary options.
- `observe`: Labels results with local/remote target, parses JSON, and detects
  missing installations separately from connection or command errors.
- `missing_result`: Returns upstream installation guidance for later shell use;
  no downloads or installation happen here.
- `run_bounded`: Drains both pipes, caps output at 256KB and runtime at 45s,
  then kills/reaps the local process group (including collector children).

## Contracts
Remote calls resolve `vz` on the remote PATH including user bin directories;
they never reuse local machine observations or copy a platform-specific binary.
The 45s bound stays below the host's 300s plugin tool deadline. Killing a local
SSH process cannot guarantee termination of every remote descendant.
Desktop metadata and recent activity are best-effort observations, not trusted
instructions or complete audit records. No continuous watcher is started.
