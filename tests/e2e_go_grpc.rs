use std::fs;
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

fn tool_in_path(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd} >/dev/null 2>&1"))
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
    dir.push(format!("nimesvc_go_grpc_e2e_{}_{}", prefix, stamp));
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

fn client_go_mod(server_dir: &PathBuf) -> String {
    format!(
        r#"module grpc-smoke-client

go 1.20

require (
    google.golang.org/grpc v1.64.0
    nimesvc/auth-grpc v0.0.0
)

replace nimesvc/auth-grpc => {}
"#,
        server_dir.display()
    )
}

fn client_main_go(port: u16) -> String {
    format!(
        r#"package main

import (
    "context"
    "fmt"

    rpc "nimesvc/auth-grpc/proto"

    "google.golang.org/grpc"
    "google.golang.org/grpc/credentials/insecure"
)

func main() {{
    conn, err := grpc.Dial("127.0.0.1:{port}", grpc.WithTransportCredentials(insecure.NewCredentials()))
    if err != nil {{
        panic(err)
    }}
    defer conn.Close()

    client := rpc.NewAuthClient(conn)
    resp, err := client.Login(context.Background(), &rpc.LoginRequest{{
        Username: "demo",
        Password: "secret",
    }})
    if err != nil {{
        panic(err)
    }}
    if resp.Value != "demo:secret" {{
        panic(fmt.Sprintf("unexpected response: %s", resp.Value))
    }}
}}
"#
    )
}

#[test]
fn go_grpc_cli_run_and_smoke_call() {
    if !tool_available("go", "version")
        || !tool_available("protoc", "--version")
        || !tool_in_path("protoc-gen-go")
        || !tool_in_path("protoc-gen-go-grpc")
    {
        return;
    }
    let Some(port) = free_port() else {
        return;
    };

    let bin = env!("CARGO_BIN_EXE_nimesvc");
    let base = temp_dir("go_grpc");
    let modules_dir = base.join("modules");
    fs::create_dir_all(&modules_dir).unwrap();
    let ns_path = base.join("api.ns");
    let out_dir = base.join("build");

    let ns = format!(
        r#"use auth "./modules/auth.go"

rpc Auth.Login:
    input:
        username: string
        password: string
    output string
    call auth.Login(input.username, input.password)

service Auth:
    grpc_config:
        address: "127.0.0.1"
        port: {port}
"#
    );
    fs::write(&ns_path, ns).unwrap();
    fs::write(
        modules_dir.join("auth.go"),
        r#"package auth

func Login(username string, password string) string {
    return username + ":" + password
}
"#,
    )
    .unwrap();

    let generate = Command::new(bin)
        .arg("generate")
        .arg(&ns_path)
        .arg("grpc")
        .arg("--lang")
        .arg("go")
        .arg("--out")
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(generate.success());

    let server_dir = out_dir.join("Auth-grpc");
    assert!(server_dir.join("go.mod").exists());
    assert!(server_dir.join("gen.sh").exists());
    assert!(server_dir.join("proto").join("rpc.proto").exists());

    let mut run = Command::new(bin)
        .arg("run")
        .arg(&ns_path)
        .arg("grpc")
        .arg("--lang")
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
        panic!("Go gRPC server did not become healthy, run status: {status}");
    }

    let client_dir = base.join("grpc-client");
    fs::create_dir_all(client_dir.join("src")).unwrap();
    fs::write(client_dir.join("go.mod"), client_go_mod(&server_dir)).unwrap();
    fs::write(client_dir.join("main.go"), client_main_go(port)).unwrap();

    let status = Command::new("go")
        .args(["run", "-mod=mod", "."])
        .current_dir(&client_dir)
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
    let _ = run.wait().unwrap();

    assert!(status.success());
}
