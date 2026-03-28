use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use nimesvc::generators::go::generate_go_server;
use nimesvc::generators::grpc::generate_grpc_server;
use nimesvc::generators::rust::generate_rust_server;
use nimesvc::generators::typescript::generate_ts_server;
use nimesvc::parser::parse_project;

use super::super::domains::prep;
use super::common::parse_lang;

pub(super) fn generate_cmd(
    kind: Option<String>,
    input: PathBuf,
    out: Option<PathBuf>,
    lang: Option<String>,
) -> Result<()> {
    let src = fs::read_to_string(&input)
        .with_context(|| format!("Failed to read input file '{}'", input.display()))?;
    let project = parse_project(&src)
        .with_context(|| format!("Failed to parse project '{}'", input.display()))?;
    let grpc_only = kind.as_deref() == Some("grpc");
    let auto_grpc = !grpc_only;
    if grpc_only {
        let lang = lang.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "`nimesvc generate {}` with kind `grpc` requires `--lang <rust|ts|go>`",
                input.display()
            )
        })?;
        let grpc_lang = parse_lang(lang)?;
        let input_dir = input
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let out_base = out
            .or_else(|| project.common.output.as_ref().map(PathBuf::from))
            .unwrap_or_else(|| input_dir.join(".nimesvc"));
        for service in &project.services {
            let out_dir = out_base.join(format!("{}-grpc", service.name));
            let prepared =
                prep::prepare_service(service, &project, &input_dir, &out_dir, &grpc_lang)
                    .with_context(|| {
                        format!(
                            "Failed to prepare gRPC service '{}' for {} generation",
                            service.name, lang
                        )
                    })?;
            fs::create_dir_all(&out_dir)
                .with_context(|| format!("Failed to create '{}'", out_dir.display()))?;
            generate_grpc_server(&prepared, &out_dir, grpc_lang.clone()).with_context(|| {
                format!(
                    "Failed to generate gRPC server for service '{}' in '{}'",
                    service.name,
                    out_dir.display()
                )
            })?;
            println!("Generated gRPC server at {}", out_dir.display());
        }
        return Ok(());
    }
    let default_lang = match (kind.as_deref(), lang.as_deref()) {
        (Some(k), _) => Some(parse_lang(k)?),
        (None, Some(l)) => Some(parse_lang(l)?),
        (None, None) => None,
    };
    let input_dir = input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let out_base = out
        .or_else(|| project.common.output.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| input_dir.join(".nimesvc"));

    for service in &project.services {
        let lang = service
            .language
            .clone()
            .or(default_lang.clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Service '{}' has no language. Pass `nimesvc generate {} <rust|ts|go>` or `--lang <rust|ts|go>`.",
                    service.name,
                    input.display()
                )
            })?;
        let out_dir = out_base.join(&service.name);
        let prepared = prep::prepare_service(service, &project, &input_dir, &out_dir, &lang)
            .with_context(|| {
                format!(
                    "Failed to prepare service '{}' for {:?} generation",
                    service.name, lang
                )
            })?;
        fs::create_dir_all(&out_dir)
            .with_context(|| format!("Failed to create '{}'", out_dir.display()))?;
        match lang {
            nimesvc::ir::Lang::Rust => {
                generate_rust_server(&prepared, &out_dir).with_context(|| {
                    format!(
                        "Failed to generate Rust server for service '{}' in '{}'",
                        service.name,
                        out_dir.display()
                    )
                })?;
                println!("Generated Rust server at {}", out_dir.display());
            }
            nimesvc::ir::Lang::TypeScript => {
                generate_ts_server(&prepared, &out_dir).with_context(|| {
                    format!(
                        "Failed to generate TypeScript server for service '{}' in '{}'",
                        service.name,
                        out_dir.display()
                    )
                })?;
                println!("Generated TypeScript server at {}", out_dir.display());
            }
            nimesvc::ir::Lang::Go => {
                generate_go_server(&prepared, &out_dir).with_context(|| {
                    format!(
                        "Failed to generate Go server for service '{}' in '{}'",
                        service.name,
                        out_dir.display()
                    )
                })?;
                println!("Generated Go server at {}", out_dir.display());
            }
        }

        if auto_grpc && !service.rpc.methods.is_empty() {
            let grpc_out_dir = out_base.join(format!("{}-grpc", service.name));
            let prepared =
                prep::prepare_service(service, &project, &input_dir, &grpc_out_dir, &lang)
                    .with_context(|| {
                        format!(
                            "Failed to prepare gRPC service '{}' for {:?} generation",
                            service.name, lang
                        )
                    })?;
            fs::create_dir_all(&grpc_out_dir)
                .with_context(|| format!("Failed to create '{}'", grpc_out_dir.display()))?;
            generate_grpc_server(&prepared, &grpc_out_dir, lang.clone()).with_context(|| {
                format!(
                    "Failed to generate gRPC server for service '{}' in '{}'",
                    service.name,
                    grpc_out_dir.display()
                )
            })?;
            println!("Generated gRPC server at {}", grpc_out_dir.display());
        }
    }
    Ok(())
}
