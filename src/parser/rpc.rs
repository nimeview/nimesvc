use anyhow::{Result, anyhow, bail};

use crate::ir::{CallArg, CallSpec, Field, InputRef, InputSource, RpcDef};

use super::model::RpcBuilder;
use super::types::{parse_name_with_version, parse_type_with_validation};

pub(super) fn parse_rpc_decl(content: &str, line_no: usize) -> Result<RpcBuilder> {
    let content = content.trim();
    if !content.starts_with("rpc ") || !content.ends_with(':') {
        bail!("Line {}: expected `rpc <Service>.<Method>:`", line_no);
    }
    let raw = content
        .strip_prefix("rpc ")
        .and_then(|s| s.strip_suffix(':'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid rpc declaration", line_no))?;

    let (svc, method) = raw
        .split_once('.')
        .ok_or_else(|| anyhow!("Line {}: expected `rpc <Service>.<Method>:`", line_no))?;
    if !super::util::is_ident(svc) {
        bail!("Line {}: invalid service name '{}'", line_no, svc);
    }
    let name = parse_name_with_version(method, line_no)?;
    Ok(RpcBuilder {
        service: svc.to_string(),
        name,
        input: Vec::new(),
        headers: Vec::new(),
        output: None,
        block: RpcBlock::None,
        call: None,
        modules: Vec::new(),
        auth: None,
        middleware: Vec::new(),
        rate_limit: None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RpcBlock {
    None,
    Input,
    Headers,
}

pub(super) fn parse_rpc_body_line(
    rpc: &mut RpcBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let content = content.trim();
    if content.starts_with("use ") {
        let module = super::use_decl::parse_use_decl(content, line_no)?;
        rpc.modules.push(module);
        return Ok(());
    }
    if content.starts_with("auth ") {
        if rpc.auth.is_some() {
            bail!("Line {}: duplicate auth directive", line_no);
        }
        rpc.auth = Some(super::use_decl::parse_auth_decl(content, line_no)?);
        return Ok(());
    }
    if content.starts_with("middleware ") {
        let mw = super::use_decl::parse_middleware_decl(content, line_no)?;
        rpc.middleware.push(mw);
        return Ok(());
    }
    if content.starts_with("rate_limit ") {
        if rpc.rate_limit.is_some() {
            bail!("Line {}: duplicate rate_limit", line_no);
        }
        let raw = content.strip_prefix("rate_limit ").unwrap().trim();
        rpc.rate_limit = Some(super::route::parse_rate_limit(raw, line_no)?);
        return Ok(());
    }
    if content.starts_with("call ") {
        if rpc.call.is_some() {
            bail!("Line {}: duplicate call", line_no);
        }
        let raw = content.strip_prefix("call ").unwrap().trim();
        rpc.call = Some(parse_rpc_call_spec(raw, line_no)?);
        return Ok(());
    }
    if content == "input:" {
        rpc.block = RpcBlock::Input;
        return Ok(());
    }
    if content == "headers:" {
        rpc.block = RpcBlock::Headers;
        return Ok(());
    }
    if content.starts_with("output ") {
        let raw = content.strip_prefix("output ").unwrap().trim();
        let (ty, _) = parse_type_with_validation(raw, line_no)?;
        rpc.output = Some(ty);
        return Ok(());
    }
    if content.starts_with("output:") {
        let raw = content.strip_prefix("output:").unwrap().trim();
        let (ty, _) = parse_type_with_validation(raw, line_no)?;
        rpc.output = Some(ty);
        return Ok(());
    }
    bail!(
        "Line {}: expected `input:`, `headers:`, `output <Type>`, or `call`",
        line_no
    );
}

pub(super) fn parse_rpc_input_field(
    rpc: &mut RpcBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    if rpc.block != RpcBlock::Input && rpc.block != RpcBlock::Headers {
        bail!("Line {}: rpc field outside input/headers block", line_no);
    }
    let content = content.trim();
    let mut parts = content.splitn(2, ':');
    let raw_name = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: missing input field name", line_no))?;
    let (name, optional) = super::types::parse_optional_name(raw_name, line_no)?;
    let ty_raw = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: missing input field type", line_no))?;
    let (ty, validation) = parse_type_with_validation(ty_raw, line_no)?;
    let field = Field {
        name,
        ty,
        optional,
        validation,
    };
    if rpc.block == RpcBlock::Input {
        rpc.input.push(field);
    } else {
        rpc.headers.push(field);
    }
    Ok(())
}

pub(super) fn finalize_rpc(rpc: Option<RpcBuilder>, line_no: usize) -> Result<Option<RpcDef>> {
    let Some(rpc) = rpc else {
        return Ok(None);
    };
    let output = rpc.output.ok_or_else(|| {
        anyhow!(
            "Line {}: rpc '{}.{}' missing output",
            line_no,
            rpc.service,
            rpc.name.display()
        )
    })?;
    let call = rpc.call.ok_or_else(|| {
        anyhow!(
            "Line {}: rpc '{}.{}' missing call",
            line_no,
            rpc.service,
            rpc.name.display()
        )
    })?;
    Ok(Some(RpcDef {
        service: rpc.service,
        name: rpc.name.name,
        version: rpc.name.version,
        input: rpc.input,
        headers: rpc.headers,
        output,
        call,
        modules: rpc.modules,
        auth: rpc.auth,
        middleware: rpc.middleware,
        rate_limit: rpc.rate_limit,
    }))
}

pub(super) fn validate_rpcs(rpcs: &[RpcDef], services: &[crate::ir::Service]) -> Result<()> {
    let svc_names: std::collections::HashSet<&str> =
        services.iter().map(|s| s.name.as_str()).collect();
    for rpc in rpcs {
        if !svc_names.contains(rpc.service.as_str()) {
            bail!("RPC references unknown service '{}'", rpc.service);
        }
        validate_rpc_call_args(rpc)?;
    }
    Ok(())
}

fn validate_rpc_call_args(rpc: &RpcDef) -> Result<()> {
    for arg in &rpc.call.args {
        match arg.value.source {
            InputSource::Input => {
                if rpc.input.is_empty() {
                    bail!("RPC '{}' uses input but no input is defined", rpc.name);
                }
                if !arg.value.path.is_empty() {
                    let first = &arg.value.path[0];
                    if !rpc.input.iter().any(|f| &f.name == first) {
                        bail!(
                            "RPC '{}' call uses unknown input field '{}'",
                            rpc.name,
                            first
                        );
                    }
                }
            }
            InputSource::Headers => {
                if rpc.headers.is_empty() {
                    bail!("RPC '{}' uses headers but no headers are defined", rpc.name);
                }
                if !arg.value.path.is_empty() {
                    let first = &arg.value.path[0];
                    if !rpc.headers.iter().any(|f| &f.name == first) {
                        bail!(
                            "RPC '{}' call uses unknown header field '{}'",
                            rpc.name,
                            first
                        );
                    }
                }
            }
            _ => {
                bail!(
                    "RPC '{}' call arguments must use input.* or headers.*",
                    rpc.name
                );
            }
        }
    }
    Ok(())
}

fn parse_rpc_call_spec(raw: &str, line_no: usize) -> Result<CallSpec> {
    let raw = raw.trim();
    let (is_async, raw) = if raw.starts_with("async ") {
        (true, raw.trim_start_matches("async ").trim())
    } else if raw.starts_with("sync ") {
        (false, raw.trim_start_matches("sync ").trim())
    } else {
        (false, raw)
    };
    let (target, args_part) = if let Some(idx) = raw.find('(') {
        if !raw.ends_with(')') {
            bail!("Line {}: call args must end with ')'", line_no);
        }
        let target = raw[..idx].trim();
        let args = raw[idx + 1..raw.len() - 1].trim();
        (target, Some(args))
    } else {
        (raw, None)
    };

    let parts: Vec<&str> = target.split('.').collect();
    let (service, module, function) = match parts.len() {
        2 => (None, parts[0].trim(), parts[1].trim()),
        3 => (Some(parts[0].trim()), parts[1].trim(), parts[2].trim()),
        _ => {
            bail!(
                "Line {}: call must be module.func or service.module.func",
                line_no
            );
        }
    };
    if let Some(svc) = service {
        if !super::util::is_ident(svc) {
            bail!("Line {}: invalid service name '{}'", line_no, svc);
        }
    }
    if !super::util::is_ident(module) || !super::util::is_ident(function) {
        bail!("Line {}: invalid call '{}'", line_no, raw);
    }

    let mut args = Vec::new();
    if let Some(args_raw) = args_part {
        if !args_raw.is_empty() {
            for chunk in args_raw.split(',') {
                let item = chunk.trim();
                if item.is_empty() {
                    continue;
                }
                let (name, expr) = if let Some(eq) = item.find('=') {
                    let name = item[..eq].trim();
                    let expr = item[eq + 1..].trim();
                    if !super::util::is_ident(name) {
                        bail!("Line {}: invalid argument name '{}'", line_no, name);
                    }
                    (Some(name.to_string()), expr)
                } else {
                    (None, item)
                };
                let value = parse_rpc_input_ref(expr, line_no)?;
                args.push(CallArg { name, value });
            }
        }
    }

    Ok(CallSpec {
        service: service.map(|s| s.to_string()),
        service_base: None,
        module: module.to_string(),
        function: function.to_string(),
        args,
        is_async,
    })
}

fn parse_rpc_input_ref(raw: &str, line_no: usize) -> Result<InputRef> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("Line {}: empty call argument", line_no);
    }
    let parts: Vec<&str> = raw.split('.').collect();
    let source = match parts[0] {
        "input" => InputSource::Input,
        "headers" => InputSource::Headers,
        _ => {
            bail!(
                "Line {}: rpc call argument must start with input.* or headers.*",
                line_no
            );
        }
    };
    let mut path = Vec::new();
    for seg in parts.iter().skip(1) {
        if !super::util::is_ident(seg) {
            bail!("Line {}: invalid field '{}'", line_no, seg);
        }
        path.push(seg.to_string());
    }
    Ok(InputRef { source, path })
}
