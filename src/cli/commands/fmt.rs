use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use nimesvc::format::format_project;
use nimesvc::parser::parse_project;

pub(super) fn fmt_cmd(input: PathBuf, check: bool) -> Result<()> {
    let src = fs::read_to_string(&input)
        .with_context(|| format!("Failed to read input file '{}'", input.display()))?;
    let project = parse_project(&src)
        .with_context(|| format!("Failed to parse project '{}'", input.display()))?;
    let formatted = format_project(&project);
    if check {
        if src == formatted {
            println!("Format: OK");
            return Ok(());
        }
        anyhow::bail!(
            "Format check failed for '{}'. Run `nimesvc fmt {}` to rewrite the file.",
            input.display(),
            input.display()
        );
    }
    fs::write(&input, formatted)
        .with_context(|| format!("Failed to write input file '{}'", input.display()))?;
    println!("Formatted {}", input.display());
    Ok(())
}
