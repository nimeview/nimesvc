use anyhow::{Result, anyhow, bail};

use crate::ir::Lang;

mod builder;
mod config;
mod versioning;

pub(crate) use builder::{
    ServiceBlock, ServiceBuilder, collect_all_rpcs, collect_known_types, finalize_service,
    validate_event_refs,
};
pub(crate) use config::{
    parse_events_config_line, parse_grpc_config_line, parse_header_field_line, parse_response_line,
    parse_service_config_line, validate_base_url,
};
pub(crate) use versioning::apply_default_version;

pub(super) fn parse_service_decl(content: &str, line_no: usize) -> Result<(String, Option<Lang>)> {
    let content = content.trim();
    if !content.starts_with("service ") || !content.ends_with(':') {
        bail!("Line {}: expected `service <Name>:`", line_no);
    }
    let raw = content
        .strip_prefix("service ")
        .and_then(|s| s.strip_suffix(':'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid service name", line_no))?;

    let mut parts = raw.split_whitespace();
    let name = parts.next().unwrap();
    let lang = parts.next().map(parse_lang).transpose()?;

    if !super::util::is_ident(name) {
        bail!("Line {}: invalid service name '{}'", line_no, name);
    }

    Ok((name.to_string(), lang))
}

fn parse_lang(raw: &str) -> Result<Lang> {
    match raw {
        "rs" | "rust" => Ok(Lang::Rust),
        "ts" | "typescript" => Ok(Lang::TypeScript),
        "go" | "golang" => Ok(Lang::Go),
        _ => bail!("Unknown service language '{}'", raw),
    }
}
