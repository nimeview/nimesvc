use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("nimesvc_ws_e2e_{}_{}", prefix, stamp));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn free_port() -> Option<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
    Some(listener.local_addr().ok()?.port())
}

fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        sleep(Duration::from_millis(100));
    }
    false
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        sleep(Duration::from_millis(100));
    }
    false
}

fn ws_connect(port: u16) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    let req = format!(
        "GET /ws HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGVzdGtleTEyMzQ1Njc4OTA=\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n"
    );
    stream.write_all(req.as_bytes()).unwrap();

    let mut resp = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = stream.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        resp.extend_from_slice(&buf[..n]);
        if resp.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let resp_text = String::from_utf8_lossy(&resp);
    assert!(
        resp_text.starts_with("HTTP/1.1 101"),
        "bad handshake: {resp_text}"
    );
    stream
}

#[test]
fn rust_websocket_generate_run_and_message_roundtrip() {
    let Some(port) = free_port() else {
        return;
    };

    let bin = env!("CARGO_BIN_EXE_nimesvc");
    let base = temp_dir("rust_ws");
    let modules_dir = base.join("modules");
    fs::create_dir_all(&modules_dir).unwrap();
    let ns_path = base.join("chat_socket.ns");
    let out_dir = base.join("build");

    let ns = format!(
        r#"service Chat rust:
    config:
        address: "127.0.0.1"
        port: {port}

    use chat "./modules/chat.rs"

    socket Chat "/ws":
        inbound:
            Join -> chat.on_join
            MessageIn -> chat.on_message
        outbound:
            MessageOut -> chat.send_message
"#
    );
    fs::write(&ns_path, ns).unwrap();
    fs::write(
        modules_dir.join("chat.rs"),
        r#"use serde_json::{json, Value};

pub async fn on_join(ctx: ChatSocketContext, _payload: Value) {
    ctx.send_raw("MessageOut", json!({ "text": "welcome" }));
}

pub async fn on_message(ctx: ChatSocketContext, payload: Value) {
    let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or("echo");
    ctx.send_raw("MessageOut", json!({ "text": text }));
}

pub async fn send_message(ctx: ChatSocketContext, payload: Value) {
    ctx.send_raw("MessageOut", payload);
}
"#,
    )
    .unwrap();

    let generate = Command::new(bin)
        .arg("generate")
        .arg(&ns_path)
        .arg("rust")
        .arg("--out")
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(generate.success());

    let mut run = Command::new(bin)
        .arg("run")
        .arg(&ns_path)
        .arg("rust")
        .arg("--out")
        .arg(&out_dir)
        .arg("--no-log")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    if !wait_for_port(port, Duration::from_secs(45)) {
        let _ = Command::new(bin)
            .arg("stop")
            .arg(&ns_path)
            .arg("--out")
            .arg(&out_dir)
            .status();
        let _ = wait_for_exit(&mut run, Duration::from_secs(5));
        let status = run.wait().unwrap();
        panic!("Rust websocket service did not become healthy, run status: {status}");
    }

    let mut ws = ws_connect(port);
    let _ = &mut ws;

    let stop = Command::new(bin)
        .arg("stop")
        .arg(&ns_path)
        .arg("--out")
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(stop.success());
    assert!(wait_for_exit(&mut run, Duration::from_secs(20)));
    let _ = run.wait().unwrap();
}
