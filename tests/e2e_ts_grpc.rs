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
    dir.push(format!("nimesvc_ts_grpc_e2e_{}_{}", prefix, stamp));
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

fn client_ts(port: u16) -> String {
    format!(
        r#"import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import path from 'path';

const protoPath = path.join(process.cwd(), 'rpc.proto');
const packageDef = protoLoader.loadSync(protoPath, {{
  keepCase: true,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
}});
const proto = grpc.loadPackageDefinition(packageDef) as any;
const AuthClient = proto.nimesvc.auth.Auth;
const client = new AuthClient(
  '127.0.0.1:{port}',
  grpc.credentials.createInsecure()
);

const response = await new Promise<any>((resolve, reject) => {{
  client.Login({{ username: 'demo', password: 'secret' }}, (err: any, resp: any) => {{
    if (err) reject(err);
    else resolve(resp);
  }});
}});

if (response.value !== 'demo:secret') {{
  throw new Error(`Unexpected response: ${{response.value}}`);
}}
"#
    )
}

#[test]
fn ts_grpc_cli_run_and_smoke_call() {
    if !tool_available("bun") {
        return;
    }
    let Some(port) = free_port() else {
        return;
    };

    let bin = env!("CARGO_BIN_EXE_nimesvc");
    let base = temp_dir("ts_grpc");
    let modules_dir = base.join("modules");
    fs::create_dir_all(&modules_dir).unwrap();
    let ns_path = base.join("api.ns");
    let out_dir = base.join("build");

    let ns = format!(
        r#"use auth "./modules/auth.ts"

rpc Auth.Login:
    input:
        username: string
        password: string
    output string
    call auth.login(input.username, input.password)

service Auth:
    grpc_config:
        address: "127.0.0.1"
        port: {port}
"#
    );
    fs::write(&ns_path, ns).unwrap();
    fs::write(
        modules_dir.join("auth.ts"),
        r#"export function login(username: string, password: string): string {
  return `${username}:${password}`;
}
"#,
    )
    .unwrap();

    let generate = Command::new(bin)
        .arg("generate")
        .arg(&ns_path)
        .arg("grpc")
        .arg("--lang")
        .arg("ts")
        .arg("--out")
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(generate.success());

    let server_dir = out_dir.join("Auth-grpc");
    assert!(server_dir.join("package.json").exists());
    assert!(server_dir.join("rpc.proto").exists());
    assert!(server_dir.join("src").join("index.ts").exists());

    let mut run = Command::new(bin)
        .arg("run")
        .arg(&ns_path)
        .arg("grpc")
        .arg("--lang")
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
            "TypeScript gRPC server did not become healthy, run status: {status}\nrun.log:\n{log}"
        );
    }

    fs::write(server_dir.join("client.ts"), client_ts(port)).unwrap();
    let status = Command::new("bun")
        .arg("run")
        .arg("client.ts")
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
