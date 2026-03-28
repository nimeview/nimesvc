use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use nimesvc::parser::parse_project;

pub(super) fn env_cmd(input: PathBuf) -> Result<()> {
    let src = fs::read_to_string(&input)
        .with_context(|| format!("Failed to read input file '{}'", input.display()))?;
    let project = parse_project(&src)?;
    if project.services.is_empty() {
        println!("No services found in {}", input.display());
        return Ok(());
    }
    let multi = project.services.len() > 1;
    let mut printed_any = false;
    for service in &project.services {
        if service.common.env.is_empty() {
            continue;
        }
        printed_any = true;
        if multi {
            println!("Service {}:", service.name);
        }
        for env in &service.common.env {
            if let Some(default) = &env.default {
                println!("  {}=\"{}\"", env.name, default);
            } else {
                println!("  {}", env.name);
            }
        }
    }
    if !printed_any {
        println!("No env vars declared in {}", input.display());
    }
    Ok(())
}
