mod api;
mod ui;

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use flume::unbounded;
use ponderer_backend::runtime::BackendRuntime;
use ponderer_backend::server::serve_backend;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

pub use ponderer_backend::character_card;
pub use ponderer_backend::comfy_client;
pub use ponderer_backend::comfy_workflow;
pub use ponderer_backend::config;

use api::ApiClient;
use config::AgentConfig;
use ui::app::AgentApp;

fn main() {
    if let Some(arg) = std::env::args().nth(1) {
        if arg == "--backend-only" {
            if let Err(error) = run_backend_only() {
                eprintln!("backend mode failed: {error:#}");
                std::process::exit(1);
            }
            return;
        }
    }

    if let Err(error) = run_desktop_mode() {
        eprintln!("startup failed: {error:#}");
        std::process::exit(1);
    }
}

fn run_desktop_mode() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,ponderer=debug")),
        )
        .init();

    tracing::info!("Ponderer frontend starting...");

    let fallback_config = AgentConfig::load();
    let mut backend_process: Option<BackendProcess> = None;

    let api_client = if should_autostart_backend() {
        let launched = launch_backend_process().context("failed to autostart backend")?;
        tracing::info!("Autostarted local backend at {}", launched.base_url);
        let client = ApiClient::new(launched.base_url.clone(), Some(launched.token.clone()));
        backend_process = Some(launched);
        client
    } else {
        tracing::info!("Using externally configured backend connection");
        ApiClient::from_env()
    };

    tracing::info!("Backend API: {}", api_client.base_url());
    if !should_autostart_backend()
        && std::env::var("PONDERER_BACKEND_TOKEN")
            .ok()
            .map(|token| token.trim().is_empty())
            .unwrap_or(true)
    {
        tracing::warn!(
            "PONDERER_BACKEND_TOKEN is unset/empty; requests will fail unless backend auth mode is disabled"
        );
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 800.0])
            .with_title("Ponderer"),
        ..Default::default()
    };

    let ui_result = eframe::run_native(
        "Ponderer",
        native_options,
        Box::new(|_cc| Ok(Box::new(AgentApp::new(api_client, fallback_config)))),
    );

    if let Some(mut backend) = backend_process {
        backend.shutdown();
    }

    if let Err(error) = ui_result {
        anyhow::bail!("UI error: {}", error);
    }

    Ok(())
}

fn run_backend_only() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,ponderer_backend=debug")),
        )
        .init();

    let config = AgentConfig::load();
    let (event_tx, event_rx) = unbounded();
    let runtime = BackendRuntime::bootstrap(config, event_tx)
        .context("failed to bootstrap backend runtime")?;

    tracing::info!("Starting embedded backend service (--backend-only)");

    let server_rt = tokio::runtime::Runtime::new().context("failed to start server runtime")?;
    server_rt.block_on(serve_backend(runtime, event_rx))
}

fn should_autostart_backend() -> bool {
    let explicit_url = std::env::var("PONDERER_BACKEND_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    if explicit_url {
        return false;
    }

    match std::env::var("PONDERER_AUTOSTART_BACKEND") {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            !(value == "0" || value == "false" || value == "no" || value == "off")
        }
        Err(_) => true,
    }
}

struct BackendProcess {
    child: Child,
    base_url: String,
    token: String,
}

impl BackendProcess {
    fn shutdown(&mut self) {
        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn launch_backend_process() -> Result<BackendProcess> {
    let bind_addr = allocate_local_bind_addr().context("failed to allocate local backend port")?;
    let token = format!("local-{}", Uuid::new_v4());
    let current_dir =
        std::env::current_dir().context("failed to read current working directory")?;

    let executable =
        std::env::current_exe().context("failed to resolve current ponderer executable path")?;

    let mut child = Command::new(executable)
        .arg("--backend-only")
        .env("PONDERER_BACKEND_BIND", bind_addr.to_string())
        .env("PONDERER_BACKEND_AUTH_MODE", "required")
        .env("PONDERER_BACKEND_TOKEN", token.clone())
        .current_dir(current_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to spawn backend process")?;

    wait_for_backend_socket_ready(bind_addr, &mut child)
        .with_context(|| format!("backend did not become ready at {}", bind_addr))?;

    Ok(BackendProcess {
        child,
        base_url: format!("http://{}", bind_addr),
        token,
    })
}

fn allocate_local_bind_addr() -> Result<SocketAddr> {
    let listener =
        TcpListener::bind("127.0.0.1:0").context("failed to bind ephemeral localhost port")?;
    let addr = listener
        .local_addr()
        .context("failed to read ephemeral listener address")?;
    drop(listener);
    Ok(addr)
}

fn wait_for_backend_socket_ready(bind_addr: SocketAddr, child: &mut Child) -> Result<()> {
    let timeout = Duration::from_secs(12);
    let start = Instant::now();

    loop {
        if let Ok(Some(status)) = child.try_wait() {
            anyhow::bail!("backend process exited early with status {}", status);
        }

        if TcpStream::connect_timeout(&bind_addr, Duration::from_millis(250)).is_ok() {
            return Ok(());
        }

        if start.elapsed() >= timeout {
            anyhow::bail!("timed out waiting for backend TCP socket");
        }

        std::thread::sleep(Duration::from_millis(80));
    }
}
