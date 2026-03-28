use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use nimesvc::openapi::generate_openapi;
use nimesvc::parser::parse_project;

use super::super::domains::fs as fs_utils;

pub(super) fn build_cmd(input: PathBuf, output: Option<PathBuf>, json: bool) -> Result<()> {
    let src = fs::read_to_string(&input)
        .with_context(|| format!("Failed to read input file '{}'", input.display()))?;
    let project = parse_project(&src)
        .with_context(|| format!("Failed to parse project '{}'", input.display()))?;
    let out_base = output
        .or_else(|| project.common.output.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));

    if project.services.len() == 1 && out_base.extension().is_some() {
        let openapi = generate_openapi(&project.services[0]);
        fs_utils::write_openapi(&openapi, &out_base, json).with_context(|| {
            format!(
                "Failed to write OpenAPI output for service '{}' to '{}'",
                project.services[0].name,
                out_base.display()
            )
        })?;
        println!("Generated {}", out_base.display());
        return Ok(());
    }

    fs::create_dir_all(&out_base)
        .with_context(|| format!("Failed to create '{}'", out_base.display()))?;
    for service in &project.services {
        let filename = if json {
            format!("openapi-{}.json", service.name)
        } else {
            format!("openapi-{}.yaml", service.name)
        };
        let out_path = out_base.join(filename);
        let openapi = generate_openapi(service);
        fs_utils::write_openapi(&openapi, &out_path, json).with_context(|| {
            format!(
                "Failed to write OpenAPI output for service '{}' to '{}'",
                service.name,
                out_path.display()
            )
        })?;
        println!("Generated {}", out_path.display());
    }
    Ok(())
}
