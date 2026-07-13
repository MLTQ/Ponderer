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
- **Does**: Initializes frontend logging, loads fallback config, reuses or autostarts a persistent authenticated local backend, builds the API client, and launches `AgentApp`. The backend survives UI exit unless UI-scoped lifetime is explicitly requested.
- **Interacts with**: `api::ApiClient`, `ponderer_backend::config::AgentConfig`, `ui::app::AgentApp`.

### `run_backend_only()`
- **Does**: Initializes backend logging and runs backend runtime + REST/WS server.
- **Interacts with**: `ponderer_backend::runtime::BackendRuntime`, `ponderer_backend::server::serve_backend`.

### `launch_backend_process()` / `wait_for_backend_socket_ready()`
- **Does**: Spawns the current executable in backend mode, injects bind/token env, and waits for local socket readiness. Persistent Unix children enter a separate process group and disconnect standard streams from the UI; UI-scoped children retain inherited output for debugging.
- **Interacts with**: local process manager, localhost networking.

### Persistent backend discovery
- **Does**: Stores a private `ponderer_backend.json` endpoint/token/PID record beside the primary config, validates its loopback-only URL and authenticated health payload, removes unreachable stale records, and reuses the living backend on later UI launches.
- **Interacts with**: `ApiClient::health`, `AgentConfig::config_path`, `PONDERER_BACKEND_DISCOVERY_FILE`.
- **Rationale**: The companion's lifetime should not be coupled to whether its window is open.
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
- The generated token is persisted with owner-only permissions on Unix so later UI processes can reconnect without spawning a second agent over the same state.
- Discovery only accepts explicit HTTP loopback IP endpoints and local API clients bypass ambient HTTP proxies so the bearer token stays on the host.
- The persistent Unix child uses a separate process group and null standard streams so closing the GUI or its launching terminal does not leave it coupled to the parent's pipes. UI-scoped mode inherits output for interactive debugging.
- Full per-user service management, bounded durable logs, upgrade handoff, and equivalent Windows lifecycle isolation remain tracked in `Ponderer-v88`.
- `ponderer_backend.launch.lock` is persistent lock metadata, not a live-PID sentinel; do not delete it while launchers may be active.
- Set `PONDERER_BACKEND_LIFETIME=ui` to restore the former behavior where a backend launched by the UI is stopped when that UI closes.
- Set `PONDERER_BACKEND_DISCOVERY_FILE` to override the local discovery record path.
- Set `PONDERER_BACKEND_URL` to use an already-running external backend.
- Set `PONDERER_AUTOSTART_BACKEND=0` to disable autostart.
