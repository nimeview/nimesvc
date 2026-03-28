use anyhow::{Result, anyhow, bail};

use super::ServiceBuilder;

use super::super::route::parse_response_spec;
use super::super::util::parse_quoted_value;
use crate::ir::{CorsConfig, HttpMethod};

pub(crate) fn validate_base_url(value: &str, line_no: usize) -> Result<String> {
    let value = value.trim();
    let valid = (value.starts_with("http://") && value.len() > "http://".len())
        || (value.starts_with("https://") && value.len() > "https://".len());
    if !valid {
        bail!(
            "Line {}: base_url must start with 'http://' or 'https://'",
            line_no
        );
    }
    Ok(value.to_string())
}

pub(crate) fn parse_service_config_line(
    service: &mut ServiceBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let mut parts = content.splitn(2, ':');
    let key = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid config key", line_no))?;
    let value = parts
        .next()
        .map(str::trim)
        .ok_or_else(|| anyhow!("Line {}: invalid config value", line_no))?;

    match key {
        "address" => {
            let addr = parse_quoted_value(value, line_no)?;
            service.address = Some(addr);
        }
        "base_url" => {
            let url = validate_base_url(&parse_quoted_value(value, line_no)?, line_no)?;
            service.base_url = Some(url);
        }
        "cors" => {
            let raw = parse_quoted_value(value, line_no)?;
            let raw = raw.trim();
            let cfg = service.cors.get_or_insert(CorsConfig {
                allow_any: false,
                origins: Vec::new(),
                methods: Vec::new(),
                headers: Vec::new(),
            });
            if raw == "*" {
                cfg.allow_any = true;
                cfg.origins.clear();
            } else {
                let origins = raw
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();
                if origins.is_empty() {
                    bail!("Line {}: cors requires '*' or a list of origins", line_no);
                }
                cfg.allow_any = false;
                cfg.origins = origins;
            }
        }
        "cors_methods" => {
            let raw = parse_quoted_value(value, line_no)?;
            let methods = raw
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| {
                    if HttpMethod::from_str(s).is_none() {
                        bail!("Line {}: invalid cors method '{}'", line_no, s);
                    }
                    Ok(s.to_string())
                })
                .collect::<Result<Vec<_>>>()?;
            if methods.is_empty() {
                bail!("Line {}: cors_methods requires a list", line_no);
            }
            let cfg = service.cors.get_or_insert(CorsConfig {
                allow_any: false,
                origins: Vec::new(),
                methods: Vec::new(),
                headers: Vec::new(),
            });
            cfg.methods = methods;
        }
        "cors_headers" => {
            let raw = parse_quoted_value(value, line_no)?;
            let headers = raw
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| {
                    if !is_valid_header_name(s) {
                        bail!("Line {}: invalid cors header '{}'", line_no, s);
                    }
                    Ok(s.to_string())
                })
                .collect::<Result<Vec<_>>>()?;
            if headers.is_empty() {
                bail!("Line {}: cors_headers requires a list", line_no);
            }
            let cfg = service.cors.get_or_insert(CorsConfig {
                allow_any: false,
                origins: Vec::new(),
                methods: Vec::new(),
                headers: Vec::new(),
            });
            cfg.headers = headers;
        }
        "port" => {
            let port: u16 = value
                .parse()
                .map_err(|_| anyhow!("Line {}: invalid port '{}'", line_no, value))?;
            if port == 0 {
                bail!("Line {}: port must be between 1 and 65535", line_no);
            }
            service.port = Some(port);
        }
        _ => bail!("Line {}: unknown config key '{}'", line_no, key),
    }
    Ok(())
}

fn is_valid_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| {
            matches!(
                b,
                b'0'..=b'9'
                    | b'a'..=b'z'
                    | b'A'..=b'Z'
                    | b'!'
                    | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
            )
        })
}

pub(crate) fn parse_grpc_config_line(
    service: &mut ServiceBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    if let Some(raw) = content.strip_prefix("tls ") {
        let cfg = service.grpc_config.get_or_insert(crate::ir::GrpcConfig {
            address: None,
            port: None,
            max_message_size: None,
            tls_cert: None,
            tls_key: None,
        });
        let mut parts = raw.split_whitespace();
        let cert = parts
            .next()
            .ok_or_else(|| anyhow!("Line {}: tls requires cert path", line_no))?;
        let key = parts
            .next()
            .ok_or_else(|| anyhow!("Line {}: tls requires key path", line_no))?;
        if parts.next().is_some() {
            bail!("Line {}: tls expects exactly two paths", line_no);
        }
        cfg.tls_cert = Some(parse_quoted_value(cert, line_no)?);
        cfg.tls_key = Some(parse_quoted_value(key, line_no)?);
        return Ok(());
    }

    let mut parts = content.splitn(2, ':');
    let key = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid grpc config key", line_no))?;
    let value = parts
        .next()
        .map(str::trim)
        .ok_or_else(|| anyhow!("Line {}: invalid grpc config value", line_no))?;

    let cfg = service.grpc_config.get_or_insert(crate::ir::GrpcConfig {
        address: None,
        port: None,
        max_message_size: None,
        tls_cert: None,
        tls_key: None,
    });

    match key {
        "address" => {
            let addr = parse_quoted_value(value, line_no)?;
            cfg.address = Some(addr);
        }
        "port" => {
            let port: u16 = value
                .parse()
                .map_err(|_| anyhow!("Line {}: invalid port '{}'", line_no, value))?;
            if port == 0 {
                bail!("Line {}: port must be between 1 and 65535", line_no);
            }
            cfg.port = Some(port);
        }
        "max_message_size" => {
            let size = parse_size_value(value, line_no)?;
            cfg.max_message_size = Some(size);
        }
        "tls" => {
            let mut parts = value.split_whitespace();
            let cert = parts
                .next()
                .ok_or_else(|| anyhow!("Line {}: tls requires cert path", line_no))?;
            let key = parts
                .next()
                .ok_or_else(|| anyhow!("Line {}: tls requires key path", line_no))?;
            if parts.next().is_some() {
                bail!("Line {}: tls expects exactly two paths", line_no);
            }
            cfg.tls_cert = Some(parse_quoted_value(cert, line_no)?);
            cfg.tls_key = Some(parse_quoted_value(key, line_no)?);
        }
        _ => bail!("Line {}: unknown grpc config key '{}'", line_no, key),
    }
    Ok(())
}

pub(crate) fn parse_events_config_line(
    service: &mut ServiceBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let mut parts = content.splitn(2, ':');
    let key = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid events config key", line_no))?;
    let value = parts
        .next()
        .map(str::trim)
        .ok_or_else(|| anyhow!("Line {}: invalid events config value", line_no))?;

    let cfg = service
        .events_config
        .get_or_insert(crate::ir::EventsConfig {
            broker: crate::ir::EventsBroker::Redis,
            url: None,
            group: None,
            consumer: None,
            stream_prefix: None,
        });

    let raw_value = if value.starts_with('"') {
        parse_quoted_value(value, line_no)?
    } else {
        value.to_string()
    };

    match key {
        "broker" => match raw_value.as_str() {
            "redis" => cfg.broker = crate::ir::EventsBroker::Redis,
            other => bail!("Line {}: unknown events broker '{}'", line_no, other),
        },
        "url" => {
            cfg.url = Some(raw_value);
        }
        "group" => {
            cfg.group = Some(raw_value);
        }
        "consumer" => {
            cfg.consumer = Some(raw_value);
        }
        "stream_prefix" => {
            cfg.stream_prefix = Some(raw_value);
        }
        _ => bail!("Line {}: unknown events config key '{}'", line_no, key),
    }
    Ok(())
}

fn parse_size_value(raw: &str, line_no: usize) -> Result<u64> {
    let raw = raw.trim().trim_matches('"');
    if raw.is_empty() {
        bail!("Line {}: invalid size", line_no);
    }
    let lower = raw.to_lowercase();
    let (num_str, mult) = if let Some(v) = lower.strip_suffix("kb") {
        (v, 1024u64)
    } else if let Some(v) = lower.strip_suffix("mb") {
        (v, 1024u64 * 1024)
    } else if let Some(v) = lower.strip_suffix("gb") {
        (v, 1024u64 * 1024 * 1024)
    } else if let Some(v) = lower.strip_suffix('b') {
        (v, 1u64)
    } else {
        (lower.as_str(), 1u64)
    };
    let value: u64 = num_str
        .trim()
        .parse()
        .map_err(|_| anyhow!("Line {}: invalid size '{}'", line_no, raw))?;
    Ok(value.saturating_mul(mult))
}

pub(crate) fn parse_header_field_line(
    target: &mut Vec<crate::ir::Field>,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let content = content.trim();
    let mut parts = content.splitn(2, ':');
    let raw_name = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: missing header name", line_no))?;
    let (name, optional) = super::super::types::parse_optional_name(raw_name, line_no)?;
    let ty_raw = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: missing header type", line_no))?;
    let (ty, validation) = super::super::types::parse_type_with_validation(ty_raw, line_no)?;
    target.push(crate::ir::Field {
        name,
        ty,
        optional,
        validation,
    });
    Ok(())
}

pub(crate) fn parse_response_line(
    content: &str,
    line_no: usize,
) -> Result<crate::ir::ResponseSpec> {
    parse_response_spec(content.trim(), line_no)
}
