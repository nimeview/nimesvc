use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn tool_available(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

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

fn ws_send_text(stream: &mut TcpStream, text: &str) {
    let payload = text.as_bytes();
    assert!(payload.len() < 126);
    let mask = [0x12, 0x34, 0x56, 0x78];
    let mut frame = Vec::with_capacity(payload.len() + 6);
    frame.push(0x81);
    frame.push(0x80 | payload.len() as u8);
    frame.extend_from_slice(&mask);
    for (idx, byte) in payload.iter().enumerate() {
        frame.push(byte ^ mask[idx % 4]);
    }
    stream.write_all(&frame).unwrap();
}

fn ws_read_text(stream: &mut TcpStream) -> String {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).unwrap();
    let opcode = header[0] & 0x0f;
    assert_eq!(opcode, 0x1, "expected text frame");
    let masked = header[1] & 0x80 != 0;
    assert!(!masked, "server frames must not be masked");
    let len = (header[1] & 0x7f) as usize;
    assert!(len < 126, "unsupported frame length");
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).unwrap();
    String::from_utf8(payload).unwrap()
}

#[test]
fn go_websocket_generate_run_and_message_roundtrip() {
    if !tool_available("go") {
        return;
    }
    let Some(port) = free_port() else {
        return;
    };

    let bin = env!("CARGO_BIN_EXE_nimesvc");
    let base = temp_dir("go_ws");
    let modules_dir = base.join("modules");
    fs::create_dir_all(&modules_dir).unwrap();
    let ns_path = base.join("chat_socket.ns");
    let out_dir = base.join("build");

    let ns = format!(
        r#"service Chat go:
    config:
        address: "127.0.0.1"
        port: {port}

    use chat "./modules/chat.go"

    socket Chat "/ws":
        inbound:
            Join -> chat.OnJoin
            MessageIn -> chat.OnMessage
        outbound:
            MessageOut -> chat.SendMessage
"#
    );
    fs::write(&ns_path, ns).unwrap();
    fs::write(
        modules_dir.join("chat.go"),
        r#"package chat

type socketSender interface {
    SendRaw(kind string, data any) error
}

func OnJoin(ctx any, _ any) {
    if s, ok := ctx.(socketSender); ok {
        _ = s.SendRaw("MessageOut", map[string]any{"text": "welcome"})
    }
}

func OnMessage(ctx any, data map[string]any) {
    text, _ := data["text"].(string)
    if text == "" {
        text = "echo"
    }
    if s, ok := ctx.(socketSender); ok {
        _ = s.SendRaw("MessageOut", map[string]any{"text": text})
    }
}

func SendMessage(ctx any, data map[string]any) {
    if s, ok := ctx.(socketSender); ok {
        _ = s.SendRaw("MessageOut", data)
    }
}
"#,
    )
    .unwrap();

    let generate = Command::new(bin)
        .arg("generate")
        .arg(&ns_path)
        .arg("go")
        .arg("--out")
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(generate.success());

    let mut run = Command::new(bin)
        .arg("run")
        .arg(&ns_path)
        .arg("go")
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
        panic!("Go websocket service did not become healthy, run status: {status}");
    }

    let mut ws = ws_connect(port);
    let first = ws_read_text(&mut ws);
    assert!(first.contains("\"type\":\"MessageOut\""));
    assert!(first.contains("\"text\":\"welcome\""));

    ws_send_text(&mut ws, r#"{"type":"MessageIn","data":{"text":"hello"}}"#);
    let second = ws_read_text(&mut ws);
    assert!(second.contains("\"type\":\"MessageOut\""));
    assert!(second.contains("\"text\":\"hello\""));

    let stop = Command::new(bin)
        .arg("stop")
        .arg(&ns_path)
        .arg("--out")
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(stop.success());
    assert!(wait_for_exit(&mut run, Duration::from_secs(20)));
    let run_status = run.wait().unwrap();
    assert!(run_status.success());
}
