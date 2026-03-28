use std::fs;
use std::io::{ErrorKind, Read, Write};
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
    dir.push(format!("nimesvc_ws_auth_e2e_{}_{}", prefix, stamp));
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

fn ws_connect(port: u16, extra_headers: &[(&str, &str)]) -> WsClient {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    let mut req = format!(
        "GET /ws HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGVzdGtleTEyMzQ1Njc4OTA=\r\n\
         Sec-WebSocket-Version: 13\r\n"
    );
    for (name, value) in extra_headers {
        req.push_str(&format!("{name}: {value}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).unwrap();

    let mut resp = Vec::new();
    let mut buf = [0u8; 1024];
    let mut header_end = None;
    loop {
        let n = stream.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        resp.extend_from_slice(&buf[..n]);
        if let Some(idx) = resp.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = Some(idx + 4);
            break;
        }
    }
    let header_end = header_end.expect("missing websocket handshake terminator");
    let resp_text = String::from_utf8_lossy(&resp[..header_end]);
    assert!(
        resp_text.starts_with("HTTP/1.1 101"),
        "bad handshake: {resp_text}"
    );
    WsClient {
        stream,
        pending: resp[header_end..].to_vec(),
    }
}

fn try_ws_read_text(client: &mut WsClient, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    let mut header = [0u8; 2];
    match try_read_exact_retry(client, &mut header, deadline) {
        Ok(true) => {}
        Ok(false) => return None,
        Err(err) => panic!("failed reading websocket frame: {err}"),
    }
    let opcode = header[0] & 0x0f;
    if opcode == 0x8 {
        return None;
    }
    assert_eq!(opcode, 0x1, "expected text frame");
    let len = (header[1] & 0x7f) as usize;
    assert!(header[1] & 0x80 == 0, "server frames must not be masked");
    assert!(len < 126, "unsupported frame length");
    let mut payload = vec![0u8; len];
    match try_read_exact_retry(client, &mut payload, deadline) {
        Ok(true) => {}
        Ok(false) => return None,
        Err(err) => panic!("failed reading websocket frame: {err}"),
    }
    Some(String::from_utf8(payload).unwrap())
}

fn try_read_exact_retry(
    client: &mut WsClient,
    mut buf: &mut [u8],
    deadline: Instant,
) -> Result<bool, std::io::Error> {
    while !buf.is_empty() {
        if !client.pending.is_empty() {
            let n = client.pending.len().min(buf.len());
            buf[..n].copy_from_slice(&client.pending[..n]);
            client.pending.drain(..n);
            let (_, rest) = buf.split_at_mut(n);
            buf = rest;
            continue;
        }
        match client.stream.read(buf) {
            Ok(0) => return Ok(false),
            Ok(n) => {
                let (_, rest) = buf.split_at_mut(n);
                buf = rest;
            }
            Err(err)
                if matches!(
                    err.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) =>
            {
                if Instant::now() >= deadline {
                    return Ok(false);
                }
                sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(err),
        }
    }
    Ok(true)
}

#[test]
fn rust_websocket_auth_and_headers_are_enforced() {
    let Some(port) = free_port() else {
        return;
    };

    let bin = env!("CARGO_BIN_EXE_nimesvc");
    let base = temp_dir("rust_ws_auth");
    let modules_dir = base.join("modules");
    fs::create_dir_all(&modules_dir).unwrap();
    let ns_path = base.join("ws_auth.ns");
    let out_dir = base.join("build");

    let ns = format!(
        r#"service SecureChat rust:
    config:
        address: "127.0.0.1"
        port: {port}
    use chat "./modules/chat.rs"

    socket Chat "/ws":
        auth bearer
        headers:
            x_trace_id: string
        inbound:
            Join -> chat.on_join
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
        panic!("Rust websocket auth service did not become healthy, run status: {status}");
    }

    let mut missing_auth = ws_connect(port, &[]);
    if let Some(first) = try_ws_read_text(&mut missing_auth, Duration::from_secs(3)) {
        assert!(first.contains("\"type\":\"Error\""));
        assert!(first.contains("\"message\":\"missing authorization\""));
    }
    assert!(
        run.try_wait().unwrap().is_none(),
        "service exited after missing auth websocket connect"
    );

    let mut missing_header = ws_connect(port, &[("Authorization", "Bearer test")]);
    if let Some(second) = try_ws_read_text(&mut missing_header, Duration::from_secs(3)) {
        assert!(second.contains("\"type\":\"Error\""));
        assert!(second.contains("\"message\":\"missing header x-trace-id\""));
    }
    assert!(
        run.try_wait().unwrap().is_none(),
        "service exited after missing header websocket connect"
    );

    let mut ok = ws_connect(
        port,
        &[("Authorization", "Bearer test"), ("x-trace-id", "abc-123")],
    );
    if let Some(third) = try_ws_read_text(&mut ok, Duration::from_secs(5)) {
        assert!(third.contains("\"type\":\"MessageOut\""));
        assert!(third.contains("\"text\":\"welcome\""));
    }
    assert!(
        run.try_wait().unwrap().is_none(),
        "service exited after valid websocket connect"
    );

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
struct WsClient {
    stream: TcpStream,
    pending: Vec<u8>,
}
