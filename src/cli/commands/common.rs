use anyhow::{Result, anyhow};

pub(super) fn parse_lang(raw: &str) -> Result<nimesvc::ir::Lang> {
    match raw {
        "rs" | "rust" => Ok(nimesvc::ir::Lang::Rust),
        "ts" | "typescript" => Ok(nimesvc::ir::Lang::TypeScript),
        "go" | "golang" => Ok(nimesvc::ir::Lang::Go),
        other => Err(anyhow!(
            "Unknown language '{}'. Supported values: rust, ts, go",
            other
        )),
    }
}
