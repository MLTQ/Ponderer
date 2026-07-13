mod api;
mod ui;

use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use flume::unbounded;
use fs2::FileExt as Fs2FileExt;
use ponderer_backend::runtime::BackendRuntime;
use ponderer_backend::server::serve_backend;
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

pub use ponderer_backend::character_card;
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
        let (client, launched) = connect_or_launch_local_backend()
            .context("failed to connect to or autostart local backend")?;
        backend_process = launched;
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
        if backend_is_ui_scoped() {
            backend.shutdown();
            remove_discovery_if_owned(backend.child.id());
        } else {
            tracing::info!(
                "Leaving local backend {} running after the UI closes",
                backend.base_url
            );
        }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct BackendDiscovery {
    base_url: String,
    token: String,
    pid: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackendLaunchLeaseRecord {
    pid: u32,
    nonce: String,
    acquired_unix_seconds: u64,
}

struct BackendLaunchLease {
    file: File,
}

impl Drop for BackendLaunchLease {
    fn drop(&mut self) {
        let _ = Fs2FileExt::unlock(&self.file);
    }
}

enum BackendLaunchLeaseOutcome {
    Acquired(BackendLaunchLease),
    Discovered(ApiClient),
}

const BACKEND_LAUNCH_LEASE_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const BACKEND_LAUNCH_LEASE_POLL_INTERVAL: Duration = Duration::from_millis(150);

impl BackendProcess {
    fn shutdown(&mut self) {
        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn connect_or_launch_local_backend() -> Result<(ApiClient, Option<BackendProcess>)> {
    if let Some(client) = connect_to_discovered_backend()? {
        tracing::info!("Reusing persistent local backend at {}", client.base_url());
        return Ok((client, None));
    }

    let _launch_lease = match acquire_backend_launch_lease_or_discover()? {
        BackendLaunchLeaseOutcome::Acquired(lease) => lease,
        BackendLaunchLeaseOutcome::Discovered(client) => {
            tracing::info!(
                "Reusing local backend launched by another UI at {}",
                client.base_url()
            );
            return Ok((client, None));
        }
    };

    // Another launcher can finish between our optimistic discovery check and
    // lease acquisition. Recheck while holding the lease before spawning.
    if let Some(client) = connect_to_discovered_backend()? {
        tracing::info!("Reusing persistent local backend at {}", client.base_url());
        return Ok((client, None));
    }

    let mut launched = launch_backend_process().context("failed to autostart backend")?;
    let client = ApiClient::new_local(launched.base_url.clone(), Some(launched.token.clone()));
    if !api_is_healthy(&client) {
        launched.shutdown();
        anyhow::bail!("new local backend failed authenticated health validation");
    }
    if let Err(error) = persist_backend_discovery(&BackendDiscovery {
        base_url: launched.base_url.clone(),
        token: launched.token.clone(),
        pid: launched.child.id(),
    }) {
        launched.shutdown();
        return Err(error).context("failed to persist local backend discovery");
    }
    tracing::info!(
        "Autostarted persistent local backend at {}",
        launched.base_url
    );
    Ok((client, Some(launched)))
}

fn acquire_backend_launch_lease_or_discover() -> Result<BackendLaunchLeaseOutcome> {
    let path = backend_launch_lease_path();
    let started = Instant::now();
    loop {
        if let Some(lease) = try_acquire_backend_launch_lease_at(&path)? {
            return Ok(BackendLaunchLeaseOutcome::Acquired(lease));
        }

        if let Some(client) = connect_to_discovered_backend()? {
            return Ok(BackendLaunchLeaseOutcome::Discovered(client));
        }

        if started.elapsed() >= BACKEND_LAUNCH_LEASE_WAIT_TIMEOUT {
            anyhow::bail!(
                "timed out waiting for another desktop process to finish launching the local backend (lease {})",
                path.display()
            );
        }
        std::thread::sleep(BACKEND_LAUNCH_LEASE_POLL_INTERVAL);
    }
}

fn try_acquire_backend_launch_lease_at(path: &Path) -> Result<Option<BackendLaunchLease>> {
    ensure_parent_directory(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("failed to open backend launch lease {}", path.display()))?;

    match Fs2FileExt::try_lock_exclusive(&file) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to lock backend launch lease {}", path.display()))
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }

    let record = BackendLaunchLeaseRecord {
        pid: std::process::id(),
        nonce: Uuid::new_v4().to_string(),
        acquired_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    let payload = serde_json::to_vec_pretty(&record)?;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&payload)?;
    file.sync_all()?;

    Ok(Some(BackendLaunchLease { file }))
}

fn backend_launch_lease_path() -> PathBuf {
    backend_discovery_path().with_extension("launch.lock")
}

fn connect_to_discovered_backend() -> Result<Option<ApiClient>> {
    let path = backend_discovery_path();
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read backend discovery {}", path.display()))
        }
    };
    let discovery = match serde_json::from_str::<BackendDiscovery>(&raw) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(
                "Ignoring malformed backend discovery file '{}': {}",
                path.display(),
                error
            );
            let _ = fs::remove_file(path);
            return Ok(None);
        }
    };
    let bind_addr = match validate_backend_discovery(&discovery) {
        Ok(bind_addr) => bind_addr,
        Err(error) => {
            tracing::warn!(
                "Ignoring unsafe backend discovery file '{}': {}",
                path.display(),
                error
            );
            let _ = fs::remove_file(path);
            return Ok(None);
        }
    };
    let client = ApiClient::new_local(discovery.base_url, Some(discovery.token));
    if api_is_healthy(&client) {
        Ok(Some(client))
    } else if TcpStream::connect_timeout(&bind_addr, Duration::from_millis(500)).is_ok() {
        anyhow::bail!(
            "discovered local backend at {} is reachable but failed authenticated health; refusing to launch a duplicate",
            client.base_url()
        )
    } else {
        tracing::warn!(
            "Removing stale backend discovery for pid {} at {}",
            discovery.pid,
            client.base_url()
        );
        let _ = fs::remove_file(path);
        Ok(None)
    }
}

fn validate_backend_discovery(discovery: &BackendDiscovery) -> Result<SocketAddr> {
    if discovery.token.trim().is_empty() {
        anyhow::bail!("discovery token is empty");
    }

    let url = reqwest::Url::parse(&discovery.base_url)
        .with_context(|| format!("invalid discovery URL '{}'", discovery.base_url))?;
    if url.scheme() != "http" {
        anyhow::bail!("local discovery URL must use http");
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        anyhow::bail!("local discovery URL must contain only a loopback host and port");
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("local discovery URL has no host"))?;
    let ip = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .with_context(|| format!("local discovery host '{host}' is not an IP address"))?;
    if !ip.is_loopback() {
        anyhow::bail!("local discovery host is not loopback");
    }
    let port = url
        .port()
        .ok_or_else(|| anyhow::anyhow!("local discovery URL has no explicit port"))?;
    Ok(SocketAddr::new(ip, port))
}

fn api_is_healthy(client: &ApiClient) -> bool {
    api_is_healthy_with_timeout(client, Duration::from_secs(2))
}

fn api_is_healthy_with_timeout(client: &ApiClient, timeout: Duration) -> bool {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::warn!("Failed to create backend discovery runtime: {}", error);
            return false;
        }
    };
    runtime.block_on(async {
        tokio::time::timeout(timeout, client.health())
            .await
            .is_ok_and(|result| result.is_ok())
    })
}

fn backend_discovery_path() -> PathBuf {
    std::env::var("PONDERER_BACKEND_DISCOVERY_FILE")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| AgentConfig::config_path().with_file_name("ponderer_backend.json"))
}

fn persist_backend_discovery(discovery: &BackendDiscovery) -> Result<()> {
    let path = backend_discovery_path();
    persist_backend_discovery_at(&path, discovery)
}

fn persist_backend_discovery_at(path: &Path, discovery: &BackendDiscovery) -> Result<()> {
    ensure_parent_directory(path)?;
    let payload = serde_json::to_vec_pretty(discovery)?;
    let temporary = path.with_extension(format!("json.{}.tmp", Uuid::new_v4()));
    let write_result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).with_context(|| {
            format!(
                "failed to create temporary backend discovery {}",
                temporary.display()
            )
        })?;
        file.write_all(&payload).with_context(|| {
            format!(
                "failed to write temporary backend discovery {}",
                temporary.display()
            )
        })?;
        file.sync_all().with_context(|| {
            format!(
                "failed to flush temporary backend discovery {}",
                temporary.display()
            )
        })?;
        drop(file);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        }
        if let Err(first_error) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(path);
            fs::rename(&temporary, path).with_context(|| {
                format!(
                    "failed to replace backend discovery {} after {}",
                    path.display(),
                    first_error
                )
            })?;
        }
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

fn ensure_parent_directory(path: &Path) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create backend state directory {}",
                parent.display()
            )
        })?;
    }
    Ok(())
}

fn remove_discovery_if_owned(pid: u32) {
    let path = backend_discovery_path();
    let belongs_to_pid = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<BackendDiscovery>(&raw).ok())
        .is_some_and(|discovery| discovery.pid == pid);
    if belongs_to_pid {
        let _ = fs::remove_file(path);
    }
}

fn backend_is_ui_scoped() -> bool {
    std::env::var("PONDERER_BACKEND_LIFETIME")
        .ok()
        .map(|value| value.trim().eq_ignore_ascii_case("ui"))
        .unwrap_or(false)
}

fn launch_backend_process() -> Result<BackendProcess> {
    let bind_addr = allocate_local_bind_addr().context("failed to allocate local backend port")?;
    let token = format!("local-{}", Uuid::new_v4());
    let current_dir =
        std::env::current_dir().context("failed to read current working directory")?;

    let executable =
        std::env::current_exe().context("failed to resolve current ponderer executable path")?;

    let ui_scoped = backend_is_ui_scoped();
    let mut command = Command::new(executable);
    command
        .arg("--backend-only")
        .env("PONDERER_BACKEND_BIND", bind_addr.to_string())
        .env("PONDERER_BACKEND_AUTH_MODE", "required")
        .env("PONDERER_BACKEND_TOKEN", token.clone())
        .current_dir(current_dir)
        .stdin(Stdio::null());

    if ui_scoped {
        command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        command.stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
    }

    let mut child = command.spawn().context("failed to spawn backend process")?;

    if let Err(error) = wait_for_backend_socket_ready(bind_addr, &mut child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error).with_context(|| format!("backend did not become ready at {bind_addr}"));
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn discovery(base_url: &str, token: &str) -> BackendDiscovery {
        BackendDiscovery {
            base_url: base_url.to_string(),
            token: token.to_string(),
            pid: 42,
        }
    }

    #[test]
    fn discovery_accepts_only_explicit_loopback_http_endpoints() {
        assert_eq!(
            validate_backend_discovery(&discovery("http://127.0.0.1:8787", "secret")).unwrap(),
            "127.0.0.1:8787".parse::<SocketAddr>().unwrap()
        );
        assert!(validate_backend_discovery(&discovery("http://[::1]:8787", "secret")).is_ok());

        for invalid in [
            discovery("https://127.0.0.1:8787", "secret"),
            discovery("http://192.0.2.4:8787", "secret"),
            discovery("http://localhost:8787", "secret"),
            discovery("http://127.0.0.1:8787/path", "secret"),
            discovery("http://127.0.0.1", "secret"),
            discovery("http://127.0.0.1:8787", ""),
        ] {
            assert!(validate_backend_discovery(&invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn health_probe_does_not_require_an_ambient_tokio_reactor() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = ApiClient::new_local(
            format!("http://{}", listener.local_addr().unwrap()),
            Some("secret".to_string()),
        );

        assert!(!api_is_healthy_with_timeout(
            &client,
            Duration::from_millis(20)
        ));
    }

    #[test]
    fn discovery_persistence_round_trips_through_a_private_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ponderer_backend.json");
        let expected = discovery("http://127.0.0.1:8787", "secret");

        persist_backend_discovery_at(&path, &expected).unwrap();

        let actual: BackendDiscovery = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn launch_lease_excludes_contenders_and_recovers_after_owner_drop() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ponderer_backend.launch.lock");

        let first = try_acquire_backend_launch_lease_at(&path)
            .unwrap()
            .expect("first lease owner");
        let first_record: BackendLaunchLeaseRecord =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(first_record.pid, std::process::id());
        assert!(try_acquire_backend_launch_lease_at(&path)
            .unwrap()
            .is_none());

        drop(first);
        let second = try_acquire_backend_launch_lease_at(&path)
            .unwrap()
            .expect("stale unlocked lease file can be reclaimed");
        let second_record: BackendLaunchLeaseRecord =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_ne!(first_record.nonce, second_record.nonce);
        drop(second);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
