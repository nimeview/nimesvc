use anyhow::{Result, anyhow, bail};

use crate::ir::{AuthSpec, ModuleUse, UseScope};

use super::util::{is_ident, is_rust_path, parse_quoted_value, split_alias, split_name_path};

pub(super) fn parse_use_decl(content: &str, line_no: usize) -> Result<ModuleUse> {
    let content = content.trim();
    let mut parts = content.splitn(2, ' ');
    let _use_kw = parts.next().unwrap();
    let rest = parts
        .next()
        .ok_or_else(|| anyhow!("Line {}: expected `use <Name>`", line_no))?
        .trim();

    let (scope, rest) = if rest.starts_with("runtime ") {
        (
            UseScope::Runtime,
            rest.trim_start_matches("runtime ").trim(),
        )
    } else if rest.starts_with("compile ") {
        (
            UseScope::Compile,
            rest.trim_start_matches("compile ").trim(),
        )
    } else {
        (UseScope::Both, rest)
    };

    let (left, alias) = if let Some((left, alias_part)) = split_alias(rest) {
        let alias = alias_part.trim();
        if !is_ident(alias) {
            bail!("Line {}: invalid alias '{}'", line_no, alias);
        }
        (left.trim(), Some(alias.to_string()))
    } else {
        (rest.trim(), None)
    };

    if let Some((name, tail)) = split_name_path(left) {
        if !is_rust_path(name) {
            bail!("Line {}: invalid module name '{}'", line_no, name);
        }
        let tail = tail.trim();
        if tail.starts_with('"') {
            let quoted = parse_quoted_value(tail, line_no)?;
            if is_path_like(&quoted) {
                Ok(ModuleUse {
                    name: name.to_string(),
                    path: Some(quoted),
                    version: None,
                    alias,
                    scope,
                })
            } else {
                Ok(ModuleUse {
                    name: name.to_string(),
                    path: None,
                    version: Some(quoted),
                    alias,
                    scope,
                })
            }
        } else {
            bail!("Line {}: invalid use syntax", line_no);
        }
    } else {
        if !is_rust_path(left) {
            bail!("Line {}: invalid module name '{}'", line_no, left);
        }
        Ok(ModuleUse {
            name: left.to_string(),
            path: None,
            version: None,
            alias,
            scope,
        })
    }
}

pub(super) fn parse_auth_decl(content: &str, line_no: usize) -> Result<AuthSpec> {
    let raw = content.strip_prefix("auth ").unwrap().trim();
    parse_auth_value(raw, line_no)
}

pub(super) fn parse_auth_value(raw: &str, line_no: usize) -> Result<AuthSpec> {
    match raw {
        "none" => Ok(AuthSpec::None),
        "bearer" => Ok(AuthSpec::Bearer),
        "api_key" => Ok(AuthSpec::ApiKey),
        _ => bail!("Line {}: invalid auth type '{}'", line_no, raw),
    }
}

pub(super) fn parse_middleware_decl(content: &str, line_no: usize) -> Result<String> {
    let raw = content.strip_prefix("middleware ").unwrap().trim();
    if raw.is_empty() {
        bail!("Line {}: invalid middleware", line_no);
    }
    Ok(raw.to_string())
}

fn is_path_like(value: &str) -> bool {
    value.starts_with('.')
        || value.starts_with('/')
        || value.contains('/')
        || value.ends_with(".rs")
        || value.ends_with(".go")
        || value.ends_with(".ts")
        || value.ends_with(".js")
}
