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
- **Does**: Initializes frontend logging, loads fallback config, autostarts backend when appropriate, builds API client, launches `AgentApp`, and shuts down launched backend on UI exit.
- **Interacts with**: `api::ApiClient`, `ponderer_backend::config::AgentConfig`, `ui::app::AgentApp`.

### `run_backend_only()`
- **Does**: Initializes backend logging and runs backend runtime + REST/WS server.
- **Interacts with**: `ponderer_backend::runtime::BackendRuntime`, `ponderer_backend::server::serve_backend`.

### `launch_backend_process()` / `wait_for_backend_socket_ready()`
- **Does**: Spawns the current executable in backend mode, injects bind/token env, and waits for local socket readiness.
- **Interacts with**: local process manager, localhost networking.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `ui::app::AgentApp` | `AgentApp::new(api_client, fallback_config)` signature | Changing constructor args breaks startup wiring |
| External backend mode | `PONDERER_BACKEND_URL` disables autostart and drives API endpoint selection | Changing env semantics without launcher updates |
| Embedded backend mode | `--backend-only` remains available and bind/token env are honored | Renaming mode flag or env contract |
| Shared config model | `AgentConfig::load()` remains available for local fallback panel state | Removing config load API |

## Notes
- Default desktop startup autostarts local backend with generated token; no manual token/code pairing is required.
- Set `PONDERER_BACKEND_URL` to use an already-running external backend.
- Set `PONDERER_AUTOSTART_BACKEND=0` to disable autostart.
