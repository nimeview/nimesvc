use anyhow::{Result, anyhow, bail};

use crate::ir::{
    CallArg, CallSpec, HttpMethod, Input, InputRef, InputSource, RateLimit, ResponseSpec, Route,
};

use super::model::{InputMode, RouteBuilder};
use super::types::parse_type_with_validation;
use super::use_decl::parse_auth_value;
use super::util::is_ident;

pub(super) fn parse_route_decl(content: &str, line_no: usize) -> Result<RouteBuilder> {
    if !content.ends_with(':') {
        bail!("Line {}: route header must end with ':'", line_no);
    }

    let header = content.trim_end_matches(':').trim();
    let mut parts = header.splitn(2, ' ');
    let method_raw = parts
        .next()
        .ok_or_else(|| anyhow!("Line {}: missing HTTP method", line_no))?;
    let method = HttpMethod::from_str(method_raw)
        .ok_or_else(|| anyhow!("Line {}: unsupported HTTP method '{}'", line_no, method_raw))?;

    let rest = parts
        .next()
        .ok_or_else(|| anyhow!("Line {}: missing path", line_no))?
        .trim();

    let path = parse_quoted_path(rest, line_no)?;

    Ok(RouteBuilder {
        method,
        path,
        input: Input::default(),
        responses: Vec::new(),
        input_mode: InputMode::None,
        auth: None,
        middleware: Vec::new(),
        call: None,
        headers: Vec::new(),
        response_mode: false,
        rate_limit: None,
        healthcheck: false,
    })
}

pub(super) fn parse_route_body_line(
    route: &mut RouteBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let content = content.trim();
    route.input_mode = InputMode::None;
    route.response_mode = false;
    if content.starts_with("response ") {
        if !route.responses.is_empty() {
            bail!("Line {}: duplicate response", line_no);
        }
        let raw = content.strip_prefix("response ").unwrap().trim();
        let spec = parse_response_spec(raw, line_no)?;
        route.responses.push(spec);
        return Ok(());
    }
    if content == "responses:" {
        if !route.responses.is_empty() {
            bail!("Line {}: duplicate responses", line_no);
        }
        route.response_mode = true;
        return Ok(());
    }

    if content.starts_with("call ") {
        if route.call.is_some() {
            bail!("Line {}: duplicate call", line_no);
        }
        let raw = content.strip_prefix("call ").unwrap().trim();
        route.call = Some(parse_call_spec(raw, line_no)?);
        return Ok(());
    }

    if content.starts_with("auth:") {
        if route.auth.is_some() {
            bail!("Line {}: duplicate auth", line_no);
        }
        let raw = content.strip_prefix("auth:").unwrap().trim();
        route.auth = Some(parse_auth_value(raw, line_no)?);
        return Ok(());
    }

    if content.starts_with("middleware:") {
        let raw = content.strip_prefix("middleware:").unwrap().trim();
        if !raw.is_empty() {
            route.middleware.push(raw.to_string());
            return Ok(());
        }
    }

    if content.starts_with("rate_limit ") {
        if route.rate_limit.is_some() {
            bail!("Line {}: duplicate rate_limit", line_no);
        }
        let raw = content.strip_prefix("rate_limit ").unwrap().trim();
        route.rate_limit = Some(parse_rate_limit(raw, line_no)?);
        return Ok(());
    }

    if content == "healthcheck" {
        if route.healthcheck {
            bail!("Line {}: duplicate healthcheck", line_no);
        }
        route.healthcheck = true;
        return Ok(());
    }

    if content == "headers:" {
        route.input_mode = InputMode::InHeaders;
        return Ok(());
    }

    if content == "input:" {
        route.input_mode = InputMode::InInput;
        return Ok(());
    }

    bail!("Line {}: unsupported route directive", line_no)
}

pub(super) fn parse_input_section_line(
    route: &mut RouteBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    if route.input_mode == InputMode::None {
        bail!("Line {}: input section requires `input:`", line_no);
    }
    let content = content.trim();
    match content {
        "path:" => route.input_mode = InputMode::InPath,
        "query:" => route.input_mode = InputMode::InQuery,
        "body:" => route.input_mode = InputMode::InBody,
        "headers:" => route.input_mode = InputMode::InHeaders,
        _ => bail!("Line {}: unsupported input section", line_no),
    }
    Ok(())
}

pub(super) fn parse_input_field_line(
    route: &mut RouteBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let section = match route.input_mode {
        InputMode::InPath | InputMode::InQuery | InputMode::InBody | InputMode::InHeaders => {
            route.input_mode
        }
        _ => bail!(
            "Line {}: input field must be inside path/query/body/headers",
            line_no
        ),
    };

    let content = content.trim();
    let mut parts = content.splitn(2, ':');
    let raw_name = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: missing field name", line_no))?;
    let (name, optional) = super::types::parse_optional_name(raw_name, line_no)?;
    let ty_raw = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: missing field type", line_no))?;
    let (ty, validation) = parse_type_with_validation(ty_raw, line_no)?;

    let field = crate::ir::Field {
        name,
        ty,
        optional,
        validation,
    };
    match section {
        InputMode::InPath => route.input.path.push(field),
        InputMode::InQuery => route.input.query.push(field),
        InputMode::InBody => route.input.body.push(field),
        InputMode::InHeaders => route.headers.push(field),
        _ => {}
    }
    Ok(())
}

pub(super) fn finalize_route(route: Option<RouteBuilder>, line_no: usize) -> Result<Option<Route>> {
    let Some(mut route) = route else {
        return Ok(None);
    };

    let call = route.call.clone();
    if route.healthcheck {
        if call.is_some() {
            bail!("Line {}: healthcheck route cannot have call", line_no);
        }
        if route.responses.is_empty() {
            route.responses.push(ResponseSpec {
                status: 200,
                ty: crate::ir::Type::String,
            });
        }
        for resp in &route.responses {
            match resp.ty {
                crate::ir::Type::String | crate::ir::Type::Void => {}
                _ => bail!(
                    "Line {}: healthcheck responses must be string or void",
                    line_no
                ),
            }
        }
    } else {
        if route.responses.is_empty() {
            bail!("Line {}: route missing response", line_no);
        }
        let call = call
            .clone()
            .ok_or_else(|| anyhow!("Line {}: route missing call", line_no))?;
        validate_call_args(&call, &route.input, &route.headers, line_no)?;
    }

    Ok(Some(Route {
        method: route.method,
        path: route.path,
        input: route.input,
        responses: route.responses,
        auth: route.auth,
        middleware: route.middleware,
        call: call.unwrap_or_else(|| CallSpec {
            service: None,
            service_base: None,
            module: "health".to_string(),
            function: "check".to_string(),
            args: Vec::new(),
            is_async: false,
        }),
        rate_limit: route.rate_limit,
        healthcheck: route.healthcheck,
        headers: route.headers,
        internal: false,
    }))
}

fn validate_call_args(
    call: &CallSpec,
    input: &Input,
    headers: &[crate::ir::Field],
    line_no: usize,
) -> Result<()> {
    for arg in &call.args {
        match arg.value.source {
            InputSource::Path => {
                if input.path.is_empty() {
                    bail!("Line {}: call uses path but no path input defined", line_no);
                }
                validate_input_path(&input.path, &arg.value.path, line_no)?;
            }
            InputSource::Query => {
                if input.query.is_empty() {
                    bail!(
                        "Line {}: call uses query but no query input defined",
                        line_no
                    );
                }
                validate_input_path(&input.query, &arg.value.path, line_no)?;
            }
            InputSource::Body => {
                if input.body.is_empty() {
                    bail!("Line {}: call uses body but no body input defined", line_no);
                }
                validate_input_path(&input.body, &arg.value.path, line_no)?;
            }
            InputSource::Headers => {
                if headers.is_empty() {
                    bail!("Line {}: call uses headers but no headers defined", line_no);
                }
                validate_input_path(headers, &arg.value.path, line_no)?;
            }
            InputSource::Input => {
                bail!(
                    "Line {}: call uses input but input is only allowed in rpc declarations",
                    line_no
                );
            }
        }
    }
    Ok(())
}

fn validate_input_path(fields: &[crate::ir::Field], path: &[String], line_no: usize) -> Result<()> {
    if path.is_empty() {
        return Ok(());
    }
    let first = &path[0];
    if !fields.iter().any(|f| &f.name == first) {
        bail!("Line {}: unknown input field '{}'", line_no, first);
    }
    Ok(())
}

fn parse_quoted_path(input: &str, line_no: usize) -> Result<String> {
    if !input.starts_with('"') || !input.ends_with('"') {
        bail!("Line {}: path must be in double quotes", line_no);
    }
    let inner = &input[1..input.len() - 1];
    if !inner.starts_with('/') {
        bail!("Line {}: path must start with '/'", line_no);
    }
    Ok(inner.to_string())
}

pub(super) fn parse_response_spec(raw: &str, line_no: usize) -> Result<ResponseSpec> {
    let parts = raw.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        bail!("Line {}: response requires a type or status", line_no);
    }
    let (status, type_part) = if is_status_code(parts[0]) {
        let status = parts[0].parse::<u16>().unwrap();
        if parts.len() == 1 {
            (status, "void".to_string())
        } else {
            (status, parts[1..].join(" "))
        }
    } else {
        (200, parts.join(" "))
    };
    let (ty, _validation) = parse_type_with_validation(type_part.trim(), line_no)?;
    Ok(ResponseSpec { status, ty })
}

fn is_status_code(raw: &str) -> bool {
    raw.len() == 3 && raw.chars().all(|c| c.is_ascii_digit())
}

fn parse_call_spec(raw: &str, line_no: usize) -> Result<CallSpec> {
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
        if !is_ident(svc) {
            bail!("Line {}: invalid service name '{}'", line_no, svc);
        }
    }
    if !is_ident(module) || !is_ident(function) {
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
                    if !is_ident(name) {
                        bail!("Line {}: invalid argument name '{}'", line_no, name);
                    }
                    (Some(name.to_string()), expr)
                } else {
                    (None, item)
                };
                let value = parse_input_ref(expr, line_no)?;
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

pub(super) fn parse_rate_limit(raw: &str, line_no: usize) -> Result<RateLimit> {
    let mut parts = raw.split('/');
    let count_str = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid rate_limit", line_no))?;
    let unit = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid rate_limit unit", line_no))?;
    if parts.next().is_some() {
        bail!("Line {}: invalid rate_limit format", line_no);
    }
    let max: u32 = count_str
        .parse()
        .map_err(|_| anyhow!("Line {}: invalid rate_limit count '{}'", line_no, count_str))?;
    if max == 0 {
        bail!("Line {}: rate_limit must be > 0", line_no);
    }
    let per_seconds = match unit {
        "s" | "sec" | "second" => 1,
        "m" | "min" | "minute" => 60,
        "h" | "hour" => 3600,
        _ => bail!("Line {}: invalid rate_limit unit '{}'", line_no, unit),
    };
    Ok(RateLimit { max, per_seconds })
}

fn parse_input_ref(raw: &str, line_no: usize) -> Result<InputRef> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("Line {}: empty call argument", line_no);
    }
    let parts: Vec<&str> = raw.split('.').collect();
    let source = match parts[0] {
        "path" => InputSource::Path,
        "query" => InputSource::Query,
        "body" => InputSource::Body,
        "headers" => InputSource::Headers,
        _ => bail!(
            "Line {}: call argument must start with path/query/body/headers",
            line_no
        ),
    };
    let mut path = Vec::new();
    for seg in parts.iter().skip(1) {
        if !is_ident(seg) {
            bail!("Line {}: invalid field '{}'", line_no, seg);
        }
        path.push(seg.to_string());
    }
    Ok(InputRef { source, path })
}
