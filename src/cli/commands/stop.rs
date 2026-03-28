use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use nimesvc::parser::parse_project;

use super::super::domains::process;

pub(super) fn stop_cmd(input: PathBuf, out: Option<PathBuf>) -> Result<()> {
    let src = fs::read_to_string(&input)
        .with_context(|| format!("Failed to read input file '{}'", input.display()))?;
    let project = parse_project(&src)
        .with_context(|| format!("Failed to parse project '{}'", input.display()))?;
    let input_dir = input
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    let out_base = out
        .or_else(|| project.common.output.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| input_dir.join(".nimesvc"));

    for service in &project.services {
        let mut stopped_any = false;

        let out_dir = out_base.join(&service.name);
        if process::stop_service(&out_dir)? {
            println!("Stopped {}", service.name);
            stopped_any = true;
        }

        if !service.rpc.methods.is_empty() {
            let grpc_out_dir = out_base.join(format!("{}-grpc", service.name));
            if process::stop_service(&grpc_out_dir)? {
                println!("Stopped {} (grpc)", service.name);
                stopped_any = true;
            }
        }

        if !stopped_any {
            println!("No running process for {}", service.name);
        }
    }
    Ok(())
}
