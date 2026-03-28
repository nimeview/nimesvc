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
    dir.push(format!("nimesvc_e2e_{}_{}", prefix, stamp));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn free_port() -> Option<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
    Some(listener.local_addr().ok()?.port())
}

fn wait_for_http_ok(port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
            let req = format!(
                "GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
            );
            if stream.write_all(req.as_bytes()).is_ok() {
                let mut resp = String::new();
                if stream.read_to_string(&mut resp).is_ok() && resp.starts_with("HTTP/1.1 200") {
                    return true;
                }
            }
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

#[test]
fn rust_cli_generate_run_stop_e2e() {
    let bin = env!("CARGO_BIN_EXE_nimesvc");
    let Some(port) = free_port() else {
        return;
    };
    let base = temp_dir("rust_cli");
    let ns_path = base.join("api.ns");
    let out_dir = base.join("build");
    let ns = format!(
        r#"service Api rust:
    config:
        address: "127.0.0.1"
        port: {port}
    GET "/health":
        response 200
        healthcheck
"#
    );
    fs::write(&ns_path, ns).unwrap();

    let generate = Command::new(bin)
        .arg("generate")
        .arg(&ns_path)
        .arg("rust")
        .arg("--out")
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(generate.success());
    assert!(out_dir.join("Api").join("Cargo.toml").exists());
    assert!(out_dir.join("Api").join("src").join("main.rs").exists());

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

    assert!(wait_for_http_ok(port, Duration::from_secs(30)));

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
