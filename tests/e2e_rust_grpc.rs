use std::fs;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn tool_available(cmd: &str, arg: &str) -> bool {
    Command::new(cmd)
        .arg(arg)
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
    dir.push(format!("nimesvc_grpc_e2e_{}_{}", prefix, stamp));
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

fn kill_child(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_run_log(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| "<no grpc run log>".to_string())
}

fn client_cargo_toml() -> &'static str {
    r#"[package]
name = "grpc-smoke-client"
version = "0.1.0"
edition = "2021"

[dependencies]
tonic = { version = "0.11", features = ["transport"] }
prost = "0.12"
tokio = { version = "1", features = ["full"] }

[build-dependencies]
tonic-build = "0.11"
"#
}

fn client_build_rs() -> &'static str {
    r#"fn main() {
    tonic_build::configure()
        .build_server(false)
        .compile(&["proto/rpc.proto"], &["proto"])
        .unwrap();
}
"#
}

fn client_main_rs(port: u16) -> String {
    format!(
        r#"pub mod rpc {{
    tonic::include_proto!("nimesvc.auth");
}}

use rpc::auth_client::AuthClient;
use rpc::LoginRequest;

#[tokio::main]
async fn main() {{
    let mut client = AuthClient::connect("http://127.0.0.1:{port}").await.unwrap();
    let response = client
        .login(tonic::Request::new(LoginRequest {{
            username: "demo".to_string(),
            password: "secret".to_string(),
        }}))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.value, "demo:secret");
}}
"#
    )
}

#[test]
fn rust_grpc_generate_and_smoke_call() {
    if !tool_available("cargo", "--version") || !tool_available("protoc", "--version") {
        return;
    }
    let Some(port) = free_port() else {
        return;
    };

    let bin = env!("CARGO_BIN_EXE_nimesvc");
    let base = temp_dir("rust_grpc");
    let modules_dir = base.join("modules");
    fs::create_dir_all(&modules_dir).unwrap();
    let ns_path = base.join("api.ns");
    let out_dir = base.join("build");

    let ns = format!(
        r#"use auth "./modules/auth.rs"

rpc Auth.Login:
    input:
        username: string
        password: string
    output string
    call async auth.login(input.username, input.password)

service Auth:
    grpc_config:
        address: "127.0.0.1"
        port: {port}
"#
    );
    fs::write(&ns_path, ns).unwrap();
    fs::write(
        modules_dir.join("auth.rs"),
        r#"pub async fn login(username: String, password: String) -> String {
    format!("{}:{}", username, password)
}
"#,
    )
    .unwrap();

    let generate = Command::new(bin)
        .arg("generate")
        .arg(&ns_path)
        .arg("grpc")
        .arg("--lang")
        .arg("rust")
        .arg("--out")
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(generate.success());

    let server_dir = out_dir.join("Auth-grpc");
    assert!(server_dir.join("Cargo.toml").exists());
    assert!(server_dir.join("rpc.proto").exists());

    let log_path = server_dir.join("grpc-run.log");
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .unwrap();
    let stderr = stdout.try_clone().unwrap();

    let server = Command::new("cargo")
        .arg("run")
        .current_dir(&server_dir)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .unwrap();

    if !wait_for_port(port, Duration::from_secs(90)) {
        kill_child(server);
        let log = read_run_log(&log_path);
        panic!("Rust gRPC server did not become healthy\nrun.log:\n{log}");
    }

    let client_dir = base.join("grpc-client");
    fs::create_dir_all(client_dir.join("src")).unwrap();
    fs::create_dir_all(client_dir.join("proto")).unwrap();
    fs::write(client_dir.join("Cargo.toml"), client_cargo_toml()).unwrap();
    fs::write(client_dir.join("build.rs"), client_build_rs()).unwrap();
    fs::write(client_dir.join("src").join("main.rs"), client_main_rs(port)).unwrap();
    fs::copy(
        server_dir.join("rpc.proto"),
        client_dir.join("proto").join("rpc.proto"),
    )
    .unwrap();

    let status = Command::new("cargo")
        .arg("run")
        .current_dir(&client_dir)
        .status()
        .unwrap();
    kill_child(server);
    assert!(status.success());
}
