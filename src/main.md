# main.rs

## Purpose
Desktop launcher entry point. Supports:
1) default desktop mode (autostarts backend + launches frontend with synchronized local auth)
2) `--backend-only` mode (runs backend server loop directly).

## Components

### `main()`
- **Does**: Dispatches runtime mode (`--backend-only` vs desktop default) and handles top-level startup failure reporting.
- **Interacts with**: `run_desktop_mode`, `run_backend_only`.

### `run_desktop_mode()`
- **Does**: Initializes frontend logging, loads fallback config, reuses or autostarts an authenticated local backend, builds the API client, and launches `AgentApp`. A backend launched by the desktop is UI-owned by default and is forcibly stopped when its frontend closes.
- **Interacts with**: `api::ApiClient`, `ponderer_backend::config::AgentConfig`, `ui::app::AgentApp`.

### `run_backend_only()`
- **Does**: Initializes backend logging and runs backend runtime + REST/WS server. UI-owned children also monitor a private stdin ownership pipe and terminate immediately at EOF.
- **Interacts with**: `ponderer_backend::runtime::BackendRuntime`, `ponderer_backend::server::serve_backend`.

### `launch_backend_process()` / `wait_for_backend_socket_ready()`
- **Does**: Spawns the current executable in backend mode, injects bind/token env, and waits for local socket readiness. UI-owned children receive a parent-death pipe; persistent Unix children enter a separate process group and disconnect standard streams from the UI.
- **Interacts with**: local process manager, localhost networking.

### `BackendProcess::shutdown()` / `Drop`
- **Does**: Immediately terminates a UI-owned backend, waits for process exit, and removes discovery only when it still belongs to that PID. The drop guard repeats this safely during error unwinding or panic.
- **Interacts with**: `run_desktop_mode`, `remove_discovery_if_owned`, `monitor_ui_parent_pipe`.
- **Rationale**: Window close is the operator's dependable emergency stop, including while a provider query is in flight.

### Local backend discovery
- **Does**: Stores a private `ponderer_backend.json` endpoint/token/PID record beside the primary config, validates its loopback-only URL and authenticated health payload, removes unreachable stale records, and reuses the living backend on later UI launches.
- **Interacts with**: `ApiClient::health`, `AgentConfig::config_path`, `PONDERER_BACKEND_DISCOVERY_FILE`.
- **Rationale**: Prevents duplicate backends during concurrent desktop launches and supports the explicit persistent-lifetime mode without weakening default UI ownership.
- **Failure behavior**: Discovery is replaced through a uniquely named private temporary file; if socket readiness, authenticated health validation, or persistence fails, the just-launched backend is stopped instead of being left undiscoverable. A reachable endpoint that fails authenticated health blocks duplicate launch rather than being treated as stale. The synchronous health probe creates its Tokio timeout inside its private runtime context, so desktop bootstrap never depends on an ambient reactor.

### Backend launch lease
- **Does**: Serializes the final discovery check and child launch across desktop processes with an OS-backed exclusive file lock. Contenders poll discovery while waiting and reuse the winner's backend as soon as it is published.
- **Interacts with**: `fs2`, `connect_to_discovered_backend`, and `connect_or_launch_local_backend`.
- **Failure behavior**: Kernel lock ownership is released on normal return, error unwinding, panic, or process death. The lease file deliberately remains as diagnostic metadata; an unlocked file is safely reclaimed without age/PID guesses.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `ui::app::AgentApp` | `AgentApp::new(api_client, fallback_config)` signature | Changing constructor args breaks startup wiring |
| External backend mode | `PONDERER_BACKEND_URL` disables autostart and drives API endpoint selection | Changing env semantics without launcher updates |
| Embedded backend mode | `--backend-only` remains available and bind/token env are honored | Renaming mode flag or env contract |
| Shared config model | `AgentConfig::load()` remains available for local fallback panel state | Removing config load API |
| Concurrent desktop launchers | Only the OS lock holder may perform the final discovery check and spawn | Deleting/replacing the lease file while a launcher is active or moving launch outside the lease scope |

## Notes
- Default desktop startup autostarts local backend with generated token; no manual token/code pairing is required.
- The generated token is persisted with owner-only permissions on Unix so concurrent UI processes do not spawn a second agent over the same state.
- Discovery only accepts explicit HTTP loopback IP endpoints and local API clients bypass ambient HTTP proxies so the bearer token stays on the host.
- UI-owned lifetime is the safe default. Normal window close forcibly stops the child, unwinding drops also stop it, and loss of the frontend process closes the ownership pipe so the backend exits even after a crash or force-close.
- Set `PONDERER_BACKEND_LIFETIME=persistent` to opt into an always-on child. Persistent Unix children use a separate process group and null standard streams so they survive the GUI and its launching terminal.
- Full per-user service management, bounded durable logs, upgrade handoff, and equivalent Windows lifecycle isolation remain tracked in `Ponderer-v88`.
- `ponderer_backend.launch.lock` is persistent lock metadata, not a live-PID sentinel; do not delete it while launchers may be active.
- `PONDERER_BACKEND_LIFETIME=ui` states the default behavior explicitly. Unknown lifetime values fail safe to UI ownership.
- Set `PONDERER_BACKEND_DISCOVERY_FILE` to override the local discovery record path.
- Set `PONDERER_BACKEND_URL` to use an already-running external backend.
- Set `PONDERER_AUTOSTART_BACKEND=0` to disable autostart.
