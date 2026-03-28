use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

use super::fs as fs_utils;

pub(crate) fn ensure_node_modules(out_dir: &PathBuf, log_enabled: bool) -> Result<()> {
    let node_modules = out_dir.join("node_modules");
    if node_modules.exists() {
        return Ok(());
    }
    if log_enabled {
        log_line(out_dir, "TypeScript: installing deps (bun install)")?;
    }
    let status = Command::new("bun")
        .arg("install")
        .current_dir(out_dir)
        .status()
        .with_context(|| {
            format!(
                "Failed to run `bun install` in '{}'. Install Bun or check that the generated TypeScript project is valid.",
                out_dir.display()
            )
        })?;
    if !status.success() {
        anyhow::bail!(
            "`bun install` failed in '{}'. Check the generated project and retry the command manually.",
            out_dir.display()
        );
    }
    Ok(())
}

pub(crate) fn ensure_go_modules(out_dir: &PathBuf, log_enabled: bool) -> Result<()> {
    let go_mod = out_dir.join("go.mod");
    if !go_mod.exists() {
        anyhow::bail!("go.mod not found in '{}'", out_dir.display());
    }
    let cache_dir = out_dir.join(".nimesvc_cache");
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("Failed to create '{}'", cache_dir.display()))?;
    let hash = fs_utils::file_hash_hex(&go_mod)?;
    let stamp = cache_dir.join("go_mod.hash");
    let need = !fs_utils::stamp_matches(&stamp, &hash)? || !out_dir.join("go.sum").exists();
    if need {
        if log_enabled {
            log_line(out_dir, "Go: tidying modules (go mod tidy)")?;
        }
        let go_bin = resolve_go_binary()?;
        let status = Command::new(go_bin)
            .args(["mod", "tidy"])
            .current_dir(out_dir)
            .status()
            .with_context(|| {
                format!(
                    "Failed to run `go mod tidy` in '{}'. Ensure Go is installed and the generated Go project is valid.",
                    out_dir.display()
                )
            })?;
        if !status.success() {
            anyhow::bail!(
                "`go mod tidy` failed in '{}'. Check the generated Go project and retry the command manually.",
                out_dir.display()
            );
        }
        fs::write(&stamp, hash)
            .with_context(|| format!("Failed to write '{}'", stamp.display()))?;
    }
    Ok(())
}

pub(crate) fn ensure_go_protoc_plugins(out_dir: &PathBuf, log_enabled: bool) -> Result<PathBuf> {
    let bin_dir = out_dir.join(".nimesvc_cache/bin");
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("Failed to create '{}'", bin_dir.display()))?;
    let bin_dir = std::fs::canonicalize(&bin_dir).unwrap_or(bin_dir);

    let go_bin = resolve_go_binary()?;
    let mut need = false;
    for tool in ["protoc-gen-go", "protoc-gen-go-grpc"] {
        if bin_dir.join(tool).is_file() {
            continue;
        }
        if which_in_path(tool).is_some() {
            continue;
        }
        need = true;
    }
    if !need {
        return Ok(bin_dir);
    }

    if log_enabled {
        log_line(out_dir, "Go gRPC: installing protoc plugins")?;
    }
    let installs = [
        "google.golang.org/protobuf/cmd/protoc-gen-go@latest",
        "google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest",
    ];
    for pkg in installs {
        let status = Command::new(&go_bin)
            .env("GOBIN", &bin_dir)
            .args(["install", pkg])
            .status()
            .with_context(|| format!("Failed to install Go protoc plugin '{}'", pkg))?;
        if !status.success() {
            anyhow::bail!(
                "Failed to install Go protoc plugin '{}'. Check your Go toolchain and network access.",
                pkg
            );
        }
    }

    Ok(bin_dir)
}

pub(crate) fn resolve_go_binary() -> Result<PathBuf> {
    if let Ok(path) = env::var("NIMESVC_GO") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    if let Some(path) = which_in_path("go") {
        return Ok(path);
    }
    for candidate in [
        "/usr/local/go/bin/go",
        "/opt/homebrew/bin/go",
        "/usr/local/bin/go",
    ] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }
    anyhow::bail!(
        "Go binary not found. Install Go or set NIMESVC_GO to the full path of the Go executable."
    );
}

pub(crate) fn ensure_rust_binary(out_dir: &PathBuf, quiet: bool) -> Result<PathBuf> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build").current_dir(out_dir);
    if quiet {
        cmd.arg("--quiet");
        cmd.env("RUSTFLAGS", "-Awarnings");
    }
    let output = cmd.output().with_context(|| {
        format!(
            "Failed to run `cargo build` in '{}'. Install Rust or check that the generated Rust project is valid.",
            out_dir.display()
        )
    })?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let mut msg = format!("`cargo build` failed in '{}'", out_dir.display());
        if !stdout.is_empty() {
            msg.push_str(&format!("\nstdout:\n{}", stdout));
        }
        if !stderr.is_empty() {
            msg.push_str(&format!("\nstderr:\n{}", stderr));
        }
        anyhow::bail!(msg);
    }

    let package = rust_package_name(out_dir)?;
    let exe_name = if cfg!(windows) {
        format!("{package}.exe")
    } else {
        package
    };
    let binary = out_dir.join("target").join("debug").join(exe_name);
    if !binary.exists() {
        anyhow::bail!(
            "Rust build succeeded, but the binary '{}' was not found",
            binary.display()
        );
    }
    std::fs::canonicalize(&binary)
        .with_context(|| format!("Failed to resolve built Rust binary '{}'", binary.display()))
}

fn rust_package_name(out_dir: &PathBuf) -> Result<String> {
    let cargo_toml = fs::read_to_string(out_dir.join("Cargo.toml"))
        .with_context(|| format!("Failed to read '{}'", out_dir.join("Cargo.toml").display()))?;
    for line in cargo_toml.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("name = ") {
            return Ok(rest.trim_matches('"').to_string());
        }
    }
    anyhow::bail!(
        "Failed to determine Rust package name from '{}'",
        out_dir.join("Cargo.toml").display()
    )
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn install_ctrlc_handler(pid_registry: Arc<Mutex<Vec<(PathBuf, u32)>>>) {
    if env::var_os("NIMESVC_DISABLE_CTRLC").is_some() {
        return;
    }
    let _ = ctrlc::set_handler(move || {
        if let Ok(guard) = pid_registry.lock() {
            for (out_dir, pid) in guard.iter() {
                let _ = kill_pid(*pid);
                let _ = remove_pid_file(out_dir);
            }
        }
        std::process::exit(0);
    });
}

pub(crate) fn stop_service(out_dir: &PathBuf) -> Result<bool> {
    let pid_path = pid_file_path(out_dir);
    if !pid_path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(&pid_path)
        .with_context(|| format!("Failed to read '{}'", pid_path.display()))?;
    let mut stopped = false;
    if let Ok(pid) = content.trim().parse::<u32>() {
        if kill_pid_quiet(pid)? {
            stopped = true;
        }
    }
    remove_pid_file(out_dir)?;
    Ok(stopped)
}

pub(crate) fn record_pid(
    out_dir: &PathBuf,
    pid: u32,
    registry: &Arc<Mutex<Vec<(PathBuf, u32)>>>,
) -> Result<()> {
    let pid_path = pid_file_path(out_dir);
    ensure_cache_dir(out_dir)?;
    fs::write(&pid_path, pid.to_string())
        .with_context(|| format!("Failed to write '{}'", pid_path.display()))?;
    if let Ok(mut guard) = registry.lock() {
        guard.push((out_dir.clone(), pid));
    }
    Ok(())
}

pub(crate) fn cleanup_pids(registry: &Arc<Mutex<Vec<(PathBuf, u32)>>>) -> Result<()> {
    if let Ok(guard) = registry.lock() {
        for (out_dir, _pid) in guard.iter() {
            remove_pid_file(out_dir)?;
        }
    }
    Ok(())
}

pub(crate) fn kill_stale_pid(out_dir: &PathBuf, log_enabled: bool) -> Result<()> {
    let pid_path = pid_file_path(out_dir);
    if !pid_path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&pid_path)
        .with_context(|| format!("Failed to read '{}'", pid_path.display()))?;
    if let Ok(pid) = content.trim().parse::<u32>() {
        if log_enabled {
            let _ = log_line(out_dir, &format!("Stopping stale process {}", pid));
        }
        let _ = kill_pid(pid);
    }
    remove_pid_file(out_dir)?;
    Ok(())
}

pub(crate) fn spawn_with_log(
    mut cmd: Command,
    out_dir: &PathBuf,
    label: &str,
    log_enabled: bool,
) -> Result<std::process::Child> {
    if log_enabled {
        let log_path = out_dir.join(".nimesvc_cache/run.log");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("Failed to open '{}'", log_path.display()))?;
        let file_err = file
            .try_clone()
            .with_context(|| format!("Failed to clone '{}'", log_path.display()))?;

        cmd.stdout(file);
        cmd.stderr(file_err);

        log_line(out_dir, &format!("Starting {} process", label))?;
    }
    cmd.spawn().with_context(|| {
        format!(
            "Failed to start {} process in '{}'",
            label,
            out_dir.display()
        )
    })
}

pub(super) fn log_line(out_dir: &PathBuf, msg: &str) -> Result<()> {
    let cache_dir = out_dir.join(".nimesvc_cache");
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("Failed to create '{}'", cache_dir.display()))?;
    let log_path = cache_dir.join("run.log");
    let ts = std::time::SystemTime::now();
    let line = format!("[{:?}] {}\n", ts, msg);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("Failed to open '{}'", log_path.display()))?
        .write_all(line.as_bytes())
        .with_context(|| format!("Failed to write '{}'", log_path.display()))?;
    Ok(())
}

fn kill_pid_quiet(pid: u32) -> Result<bool> {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .with_context(|| format!("Failed to send SIGTERM to process {}", pid))?;
    Ok(status.success())
}

fn kill_pid(pid: u32) -> Result<()> {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .with_context(|| format!("Failed to send SIGTERM to process {}", pid))?;
    if !status.success() {
        anyhow::bail!("Failed to stop process {}", pid);
    }
    Ok(())
}

fn pid_file_path(out_dir: &PathBuf) -> PathBuf {
    out_dir.join(".nimesvc_cache/service.pid")
}

fn remove_pid_file(out_dir: &PathBuf) -> Result<()> {
    let pid_path = pid_file_path(out_dir);
    if pid_path.exists() {
        fs::remove_file(&pid_path)
            .with_context(|| format!("Failed to remove '{}'", pid_path.display()))?;
    }
    Ok(())
}

fn ensure_cache_dir(out_dir: &PathBuf) -> Result<()> {
    let cache_dir = out_dir.join(".nimesvc_cache");
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("Failed to create '{}'", cache_dir.display()))?;
    Ok(())
}
