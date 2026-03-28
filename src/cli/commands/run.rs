use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

use nimesvc::generators::go::generate_go_server;
use nimesvc::generators::grpc::generate_grpc_server;
use nimesvc::generators::rust::generate_rust_server;
use nimesvc::generators::typescript::generate_ts_server;
use nimesvc::parser::parse_project;

use super::super::domains::prep;
use super::super::domains::process;
use super::common::parse_lang;

pub(super) fn run_cmd(
    kind: Option<String>,
    lang: Option<String>,
    no_log: bool,
    input: PathBuf,
    out: Option<PathBuf>,
) -> Result<()> {
    let skip_generate = std::env::var_os("NIMESVC_SKIP_GENERATE").is_some();
    let quiet_rust_runtime = std::env::var_os("NIMESVC_DEV_RUNTIME").is_some();
    let src = fs::read_to_string(&input)
        .with_context(|| format!("Failed to read input file '{}'", input.display()))?;
    let project = parse_project(&src)
        .with_context(|| format!("Failed to parse project '{}'", input.display()))?;
    let grpc_only = matches!(kind.as_deref(), Some("grpc"));
    let auto_grpc = !grpc_only;
    let grpc_lang = if grpc_only {
        let raw = lang.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "`nimesvc run {}` with kind `grpc` requires `--lang <rust|ts|go>`",
                input.display()
            )
        })?;
        Some(parse_lang(raw)?)
    } else {
        None
    };
    let default_lang = if grpc_only {
        None
    } else {
        match (kind.as_deref(), lang.as_deref()) {
            (Some(k), _) => Some(parse_lang(k)?),
            (None, Some(l)) => Some(parse_lang(l)?),
            (None, None) => None,
        }
    };
    let input_dir = input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let out_base = out
        .or_else(|| project.common.output.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| input_dir.join(".nimesvc"));

    let mut children: Vec<(String, PathBuf, std::process::Child)> = Vec::new();
    let pid_registry: Arc<Mutex<Vec<(PathBuf, u32)>>> = Arc::new(Mutex::new(Vec::new()));
    process::install_ctrlc_handler(pid_registry.clone());
    let mut grpc_started = false;
    for service in &project.services {
        if grpc_only {
            let lang = grpc_lang.clone().unwrap();
            if service.rpc.methods.is_empty() {
                continue;
            }
            grpc_started = true;
            let out_dir = out_base.join(format!("{}-grpc", service.name));
            let prepared = prep::prepare_service(service, &project, &input_dir, &out_dir, &lang)?;
            fs::create_dir_all(&out_dir)
                .with_context(|| format!("Failed to create '{}'", out_dir.display()))?;
            process::kill_stale_pid(&out_dir, !no_log)?;
            if !skip_generate {
                generate_grpc_server(&prepared, &out_dir, lang.clone())?;
            }
            match lang {
                nimesvc::ir::Lang::Rust => {
                    let child = if quiet_rust_runtime {
                        let binary = process::ensure_rust_binary(&out_dir, true)?;
                        Command::new(binary)
                            .current_dir(&out_dir)
                            .spawn()
                            .with_context(|| {
                                format!(
                                    "Failed to start Rust gRPC service '{}' in '{}'",
                                    service.name,
                                    out_dir.display()
                                )
                            })?
                    } else {
                        Command::new("cargo")
                            .arg("run")
                            .current_dir(&out_dir)
                            .spawn()
                            .with_context(|| {
                                format!(
                                    "Failed to start `cargo run` for gRPC service '{}' in '{}'",
                                    service.name,
                                    out_dir.display()
                                )
                            })?
                    };
                    process::record_pid(&out_dir, child.id(), &pid_registry)?;
                    children.push((service.name.clone(), out_dir.clone(), child));
                }
                nimesvc::ir::Lang::TypeScript => {
                    process::ensure_node_modules(&out_dir, !no_log)?;
                    let mut cmd = Command::new("bun");
                    cmd.arg("run").arg("dev").current_dir(&out_dir);
                    let child = process::spawn_with_log(cmd, &out_dir, "ts", !no_log)?;
                    process::record_pid(&out_dir, child.id(), &pid_registry)?;
                    children.push((service.name.clone(), out_dir.clone(), child));
                }
                nimesvc::ir::Lang::Go => {
                    process::ensure_go_modules(&out_dir, !no_log)?;
                    let plugin_bin = process::ensure_go_protoc_plugins(&out_dir, !no_log)?;
                    let path = std::env::var("PATH").unwrap_or_default();
                    let path = format!("{}:{}", plugin_bin.display(), path);
                    let status = Command::new("bash")
                        .arg("gen.sh")
                        .env("PATH", &path)
                        .current_dir(&out_dir)
                        .status()
                        .with_context(|| {
                            format!(
                                "Failed to run gRPC code generation script '{}' for service '{}'",
                                out_dir.join("gen.sh").display(),
                                service.name
                            )
                        })?;
                    if !status.success() {
                        anyhow::bail!(
                            "gRPC code generation failed for service '{}'. Check that `protoc`, `protoc-gen-go`, and `protoc-gen-go-grpc` are installed.",
                            service.name
                        );
                    }
                    let go_bin = process::resolve_go_binary()?;
                    let mut cmd = Command::new(go_bin);
                    cmd.arg("run")
                        .arg("-mod=mod")
                        .arg(".")
                        .env("PATH", &path)
                        .current_dir(&out_dir);
                    let child = process::spawn_with_log(cmd, &out_dir, "go", !no_log)?;
                    process::record_pid(&out_dir, child.id(), &pid_registry)?;
                    children.push((service.name.clone(), out_dir.clone(), child));
                }
            }
        } else {
            let lang = service
                .language
                .clone()
                .or(default_lang.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Service '{}' has no language. Pass `nimesvc run {} <rust|ts|go>` or `--lang <rust|ts|go>`.",
                        service.name,
                        input.display()
                    )
                })?;
            let out_dir = out_base.join(&service.name);
            let prepared = prep::prepare_service(service, &project, &input_dir, &out_dir, &lang)?;
            fs::create_dir_all(&out_dir)
                .with_context(|| format!("Failed to create '{}'", out_dir.display()))?;
            process::kill_stale_pid(&out_dir, !no_log)?;
            match lang {
                nimesvc::ir::Lang::Rust => {
                    if !skip_generate {
                        generate_rust_server(&prepared, &out_dir)?;
                    }
                    let child = if quiet_rust_runtime {
                        let binary = process::ensure_rust_binary(&out_dir, true)?;
                        Command::new(binary)
                            .current_dir(&out_dir)
                            .spawn()
                            .with_context(|| {
                                format!(
                                    "Failed to start Rust service '{}' in '{}'",
                                    service.name,
                                    out_dir.display()
                                )
                            })?
                    } else {
                        Command::new("cargo")
                            .arg("run")
                            .current_dir(&out_dir)
                            .spawn()
                            .with_context(|| {
                                format!(
                                    "Failed to start `cargo run` for service '{}' in '{}'",
                                    service.name,
                                    out_dir.display()
                                )
                            })?
                    };
                    process::record_pid(&out_dir, child.id(), &pid_registry)?;
                    children.push((service.name.clone(), out_dir.clone(), child));
                }
                nimesvc::ir::Lang::TypeScript => {
                    if !skip_generate {
                        generate_ts_server(&prepared, &out_dir)?;
                    }
                    process::ensure_node_modules(&out_dir, !no_log)?;
                    let mut cmd = Command::new("bun");
                    cmd.arg("run").arg("dev").current_dir(&out_dir);
                    let child = process::spawn_with_log(cmd, &out_dir, "ts", !no_log)?;
                    process::record_pid(&out_dir, child.id(), &pid_registry)?;
                    children.push((service.name.clone(), out_dir.clone(), child));
                }
                nimesvc::ir::Lang::Go => {
                    if !skip_generate {
                        generate_go_server(&prepared, &out_dir)?;
                    }
                    process::ensure_go_modules(&out_dir, !no_log)?;
                    let go_bin = process::resolve_go_binary()?;
                    let mut cmd = Command::new(go_bin);
                    cmd.arg("run")
                        .arg("-mod=mod")
                        .arg(".")
                        .current_dir(&out_dir);
                    let child = process::spawn_with_log(cmd, &out_dir, "go", !no_log)?;
                    process::record_pid(&out_dir, child.id(), &pid_registry)?;
                    children.push((service.name.clone(), out_dir.clone(), child));
                }
            }

            if auto_grpc && !service.rpc.methods.is_empty() {
                grpc_started = true;
                let grpc_out_dir = out_base.join(format!("{}-grpc", service.name));
                let prepared =
                    prep::prepare_service(service, &project, &input_dir, &grpc_out_dir, &lang)?;
                fs::create_dir_all(&grpc_out_dir)
                    .with_context(|| format!("Failed to create '{}'", grpc_out_dir.display()))?;
                process::kill_stale_pid(&grpc_out_dir, !no_log)?;
                if !skip_generate {
                    generate_grpc_server(&prepared, &grpc_out_dir, lang.clone())?;
                }
                match lang {
                    nimesvc::ir::Lang::Rust => {
                        let child = if quiet_rust_runtime {
                            let binary = process::ensure_rust_binary(&grpc_out_dir, true)?;
                            Command::new(binary)
                                .current_dir(&grpc_out_dir)
                                .spawn()
                                .with_context(|| {
                                    format!(
                                        "Failed to start Rust gRPC service '{}' in '{}'",
                                        service.name,
                                        grpc_out_dir.display()
                                    )
                                })?
                        } else {
                            Command::new("cargo")
                                .arg("run")
                                .current_dir(&grpc_out_dir)
                                .spawn()
                                .with_context(|| {
                                    format!(
                                        "Failed to start `cargo run` for gRPC service '{}' in '{}'",
                                        service.name,
                                        grpc_out_dir.display()
                                    )
                                })?
                        };
                        process::record_pid(&grpc_out_dir, child.id(), &pid_registry)?;
                        children.push((
                            format!("{} (grpc)", service.name),
                            grpc_out_dir.clone(),
                            child,
                        ));
                    }
                    nimesvc::ir::Lang::TypeScript => {
                        process::ensure_node_modules(&grpc_out_dir, !no_log)?;
                        let mut cmd = Command::new("bun");
                        cmd.arg("run").arg("dev").current_dir(&grpc_out_dir);
                        let child = process::spawn_with_log(cmd, &grpc_out_dir, "ts", !no_log)?;
                        process::record_pid(&grpc_out_dir, child.id(), &pid_registry)?;
                        children.push((
                            format!("{} (grpc)", service.name),
                            grpc_out_dir.clone(),
                            child,
                        ));
                    }
                    nimesvc::ir::Lang::Go => {
                        process::ensure_go_modules(&grpc_out_dir, !no_log)?;
                        let plugin_bin = process::ensure_go_protoc_plugins(&grpc_out_dir, !no_log)?;
                        let path = std::env::var("PATH").unwrap_or_default();
                        let path = format!("{}:{}", plugin_bin.display(), path);
                        let status = Command::new("bash")
                            .arg("gen.sh")
                            .env("PATH", &path)
                            .current_dir(&grpc_out_dir)
                            .status()
                            .with_context(|| {
                                format!(
                                    "Failed to run gRPC code generation script '{}' for service '{}'",
                                    grpc_out_dir.join("gen.sh").display(),
                                    service.name
                                )
                            })?;
                        if !status.success() {
                            anyhow::bail!(
                                "gRPC code generation failed for service '{}'. Check that `protoc`, `protoc-gen-go`, and `protoc-gen-go-grpc` are installed.",
                                service.name
                            );
                        }
                        let go_bin = process::resolve_go_binary()?;
                        let mut cmd = Command::new(go_bin);
                        cmd.arg("run")
                            .arg("-mod=mod")
                            .arg(".")
                            .env("PATH", &path)
                            .current_dir(&grpc_out_dir);
                        let child = process::spawn_with_log(cmd, &grpc_out_dir, "go", !no_log)?;
                        process::record_pid(&grpc_out_dir, child.id(), &pid_registry)?;
                        children.push((
                            format!("{} (grpc)", service.name),
                            grpc_out_dir.clone(),
                            child,
                        ));
                    }
                }
            }
        }
    }
    if grpc_only && !grpc_started {
        anyhow::bail!("No RPC services found in '{}'", input.display());
    }
    let mut wait_error: Option<anyhow::Error> = None;
    for (name, out_dir, mut child) in children {
        let status = child
            .wait()
            .with_context(|| format!("Failed waiting for service '{}'", name))?;
        if !status.success() {
            let pid_path = out_dir.join(".nimesvc_cache/service.pid");
            if pid_path.exists() {
                wait_error = Some(anyhow::anyhow!(
                    "Service '{}' exited with a non-zero status. Check '{}'.",
                    name,
                    out_dir.join(".nimesvc_cache/run.log").display()
                ));
            }
        }
    }
    process::cleanup_pids(&pid_registry)?;
    if let Some(err) = wait_error {
        return Err(err);
    }
    Ok(())
}
