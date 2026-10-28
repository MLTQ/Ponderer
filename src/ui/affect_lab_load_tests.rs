use super::*;
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

// Exercise the real client/controller sequence with no model or user config.
// In particular, activation must continue after navigating away from Settings.
fn run_load_flow(phase: &str) -> (Vec<String>, Option<AgentConfig>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = ApiClient::new_local(format!("http://{}", listener.local_addr().unwrap()), None);
    let paths = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicBool::new(false));
    let server_paths = paths.clone();
    let server_done = done.clone();
    let phase = phase.to_owned();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        while !server_done.load(Ordering::Relaxed) && Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut input = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            input.read_line(&mut line).unwrap();
            let path = line.split_whitespace().nth(1).unwrap().to_owned();
            let mut length = 0;
            loop {
                line.clear();
                assert!(
                    input.read_line(&mut line).unwrap() > 0,
                    "Incomplete HTTP headers"
                );
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            input.read_exact(&mut vec![0; length]).unwrap();
            server_paths.lock().unwrap().push(path.clone());
            let response = match path.as_str() {
                "/v1/affect-lab/start" => json!({"running":true}),
                "/v1/affect-lab/load" => json!({"running":true,"job":{"phase":"running"}}),
                "/v1/affect-lab" if phase == "complete" => json!({"running":true,"native_pid":123,"applied_profile":{},"job":{"phase":"complete"}}),
                "/v1/affect-lab" => json!({"running":true,"native_pid":null,"job":{"phase":phase}}),
                "/v1/affect-lab/use-for-agent" => {
                    let config = AgentConfig { llm_model: "ponderer-local-gguf".into(), ..Default::default() };
                    serde_json::to_value(config).unwrap()
                }
                other => panic!("Unexpected request: {other}"),
            }.to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
        }
    });
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut panel = AffectLabPanel::new();
    panel.request(&client, &runtime, "start", json!({}));
    assert!(panel.provider_change_pending());
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut provider = None;
    while Instant::now() < deadline {
        panel.last_poll = Instant::now() - Duration::from_secs(2);
        provider = panel.tick(&client, &runtime, false);
        if provider.is_some() || (!panel.activate_after_load && panel.pending_action.is_none()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    done.store(true, Ordering::Relaxed);
    server.join().unwrap();
    assert!(
        !panel.provider_change_pending(),
        "Load/activation did not finish"
    );
    let paths = paths.lock().unwrap().clone();
    (paths, provider)
}

#[test]
fn load_selects_chat_after_success_even_outside_settings() {
    let (paths, config) = run_load_flow("complete");
    assert_eq!(config.unwrap().llm_model, "ponderer-local-gguf");
    assert_eq!(
        &paths[..4],
        [
            "/v1/affect-lab/start",
            "/v1/affect-lab/load",
            "/v1/affect-lab",
            "/v1/affect-lab/use-for-agent"
        ]
    );
    assert_eq!(
        paths
            .iter()
            .filter(|p| p.ends_with("use-for-agent"))
            .count(),
        1
    );
}

#[test]
fn failed_or_cancelled_load_never_changes_chat_provider() {
    for phase in ["failed", "cancelled"] {
        let (paths, config) = run_load_flow(phase);
        assert!(config.is_none());
        assert!(!paths.iter().any(|p| p.ends_with("use-for-agent")));
    }
}

#[test]
fn stale_ready_status_cannot_activate_a_model() {
    let mut panel = AffectLabPanel::new();
    panel.epoch = 2;
    panel.activate_after_load = true;
    panel.handle_reply(LabReply { epoch:1, poll:true, action:"status".into(), mix_version:0,
        result:Ok(LabResult::Status(json!({"running":true,"native_pid":123,"applied_profile":{},"job":{"phase":"complete"}}))) });
    assert!(!panel.ready_for_session());
    assert!(panel.activate_after_load);
}
