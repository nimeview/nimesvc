use std::fs;
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
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
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

fn read_run_log(service_dir: &PathBuf) -> String {
    fs::read_to_string(service_dir.join(".nimesvc_cache").join("run.log"))
        .unwrap_or_else(|_| "<no run log>".to_string())
}

fn ws_client_script(port: u16) -> String {
    format!(
        r#"const ws = new WebSocket('ws://127.0.0.1:{port}/ws');

const messages = [];
const done = new Promise((resolve, reject) => {{
  const timer = setTimeout(() => reject(new Error('websocket timeout')), 10000);

  ws.onopen = () => {{
    ws.send(JSON.stringify({{ type: 'MessageIn', data: {{ text: 'hello' }} }}));
  }};

  ws.onmessage = (event) => {{
    const msg = JSON.parse(String(event.data));
    messages.push(msg);
    const hasWelcome = messages.some((m) => m.type === 'MessageOut' && m.data?.text === 'welcome');
    const hasEcho = messages.some((m) => m.type === 'MessageOut' && m.data?.text === 'hello');
    if (hasWelcome && hasEcho) {{
      clearTimeout(timer);
      ws.close();
      resolve(null);
    }}
  }};

  ws.onerror = () => {{
    clearTimeout(timer);
    reject(new Error('websocket client error'));
  }};
}});

await done;
"#
    )
}

#[test]
fn ts_websocket_generate_run_and_message_roundtrip() {
    if !tool_available("bun") {
        return;
    }
    let Some(port) = free_port() else {
        return;
    };

    let bin = env!("CARGO_BIN_EXE_nimesvc");
    let base = temp_dir("ts_ws");
    let modules_dir = base.join("examples").join("modules");
    fs::create_dir_all(&modules_dir).unwrap();
    let ns_path = base.join("chat_socket.ns");
    let out_dir = base.join("build");

    let ns = format!(
        r#"service Chat ts:
    config:
        address: "127.0.0.1"
        port: {port}

    use chat "./examples/modules/chat.ts"

    socket Chat "/ws":
        inbound:
            Join -> chat.onJoin
            MessageIn -> chat.onMessage
        outbound:
            MessageOut -> chat.sendMessage
"#
    );
    fs::write(&ns_path, ns).unwrap();
    fs::write(
        modules_dir.join("chat.ts"),
        r#"export async function onJoin(ctx: any, _payload: any) {
  ctx.sendRaw('MessageOut', { text: 'welcome' });
}

export async function onMessage(ctx: any, payload: any) {
  ctx.sendRaw('MessageOut', { text: payload?.text || 'echo' });
}

export async function sendMessage(ctx: any, payload: any) {
  ctx.sendRaw('MessageOut', payload);
}
"#,
    )
    .unwrap();

    let generate = Command::new(bin)
        .arg("generate")
        .arg(&ns_path)
        .arg("ts")
        .arg("--out")
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(generate.success());

    let server_dir = out_dir.join("Chat");
    assert!(server_dir.join("package.json").exists());
    assert!(server_dir.join("src").join("index.ts").exists());

    let mut run = Command::new(bin)
        .arg("run")
        .arg(&ns_path)
        .arg("ts")
        .arg("--out")
        .arg(&out_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    if !wait_for_port(port, Duration::from_secs(90)) {
        let _ = Command::new(bin)
            .arg("stop")
            .arg(&ns_path)
            .arg("--out")
            .arg(&out_dir)
            .status();
        let _ = wait_for_exit(&mut run, Duration::from_secs(5));
        let status = run.wait().unwrap();
        let log = read_run_log(&server_dir);
        panic!(
            "TypeScript websocket service did not become healthy, run status: {status}\nrun.log:\n{log}"
        );
    }

    fs::write(server_dir.join("ws-client.ts"), ws_client_script(port)).unwrap();
    let status = Command::new("bun")
        .arg("run")
        .arg("ws-client.ts")
        .current_dir(&server_dir)
        .status()
        .unwrap();

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

    assert!(status.success());
    assert!(run_status.success());
}
