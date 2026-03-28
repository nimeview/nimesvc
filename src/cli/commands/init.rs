use std::fs;

use anyhow::{Context, Result};

pub(super) fn init_cmd() -> Result<()> {
    let cwd = std::env::current_dir().with_context(|| "Failed to resolve current directory")?;
    let nimesvc_dir = cwd.join(".nimesvc");
    fs::create_dir_all(&nimesvc_dir)
        .with_context(|| format!("Failed to create '{}'", nimesvc_dir.display()))?;

    let main_path = cwd.join("main.ns");
    if main_path.exists() {
        anyhow::bail!("{} already exists", main_path.display());
    }
    let content = r#"service API:
    GET "/":
        response 200
        healthcheck
"#;
    fs::write(&main_path, content)
        .with_context(|| format!("Failed to write '{}'", main_path.display()))?;
    println!("Created {}", main_path.display());
    println!("Created {}", nimesvc_dir.display());
    Ok(())
}
