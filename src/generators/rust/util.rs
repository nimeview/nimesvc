use crate::generators::common::types::uses_named_type as common_uses_named_type;
use crate::ir::{EventsBroker, HttpMethod, ResponseSpec, Route, Service, Type, TypeName};

pub(super) fn build_cargo_toml(service: &Service, crate_name: &str) -> String {
    let axum_dep = if service.sockets.sockets.is_empty() {
        "\"0.7\"".to_string()
    } else {
        "{ version = \"0.7\", features = [\"ws\"] }".to_string()
    };
    let mut deps = vec![
        ("axum".to_string(), axum_dep),
        (
            "tokio".to_string(),
            "{ version = \"1\", features = [\"full\"] }".to_string(),
        ),
        (
            "serde".to_string(),
            "{ version = \"1\", features = [\"derive\"] }".to_string(),
        ),
        ("serde_json".to_string(), "\"1.0\"".to_string()),
    ];
    if needs_remote_calls(service) {
        deps.push((
            "reqwest".to_string(),
            "{ version = \"0.11\", features = [\"json\"] }".to_string(),
        ));
    }
    if !service.events.definitions.is_empty() {
        deps.push(("once_cell".to_string(), "\"1.19\"".to_string()));
        if matches!(
            service.events.config.as_ref().map(|c| &c.broker),
            Some(EventsBroker::Redis)
        ) {
            deps.push((
                "redis".to_string(),
                "{ version = \"0.25\", features = [\"tokio-comp\"] }".to_string(),
            ));
        }
    }
    if needs_regex(service) {
        deps.push(("regex".to_string(), "\"1.10\"".to_string()));
    }
    if service.common.cors.is_some() {
        deps.push((
            "tower-http".to_string(),
            "{ version = \"0.5\", features = [\"cors\"] }".to_string(),
        ));
    }
    if !service.sockets.sockets.is_empty() {
        deps.push((
            "futures-util".to_string(),
            "{ version = \"0.3\", features = [\"sink\"] }".to_string(),
        ));
    }

    for module in &service.common.modules {
        if module.path.is_none() {
            let crate_name = module.name.split("::").next().unwrap().to_string();
            if deps.iter().any(|(name, _)| name == &crate_name) {
                continue;
            }
            let version = module.version.clone().unwrap_or_else(|| "*".to_string());
            let value = if version == "*" {
                "\"*\"".to_string()
            } else {
                format!("\"{}\"", version)
            };
            deps.push((crate_name, value));
        }
    }

    let mut dep_lines = String::new();
    for (name, value) in deps {
        dep_lines.push_str(&format!("{} = {}\n", name, value));
    }

    format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
{dep_lines}"#
    )
}

pub(super) fn needs_remote_calls(service: &Service) -> bool {
    service
        .http
        .routes
        .iter()
        .any(|r| r.call.service_base.is_some())
}

fn needs_regex(service: &Service) -> bool {
    service.http.routes.iter().any(|r| {
        r.input
            .path
            .iter()
            .chain(r.input.query.iter())
            .chain(r.input.body.iter())
            .chain(r.headers.iter())
            .any(|f| {
                f.validation
                    .as_ref()
                    .map(|v| v.regex.is_some() || v.format.is_some())
                    .unwrap_or(false)
            })
    })
}

pub(super) fn rust_type(ty: &Type) -> String {
    match ty {
        Type::String => "String".to_string(),
        Type::Int => "i64".to_string(),
        Type::Float => "f64".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Object(_) => "serde_json::Value".to_string(),
        Type::Array(inner) => format!("Vec<{}>", rust_type(inner)),
        Type::Map(inner) => format!("std::collections::HashMap<String, {}>", rust_type(inner)),
        Type::Union(_) | Type::OneOf(_) => "serde_json::Value".to_string(),
        Type::Nullable(inner) => format!("Option<{}>", rust_type(inner)),
        Type::Any => "serde_json::Value".to_string(),
        Type::Void => "()".to_string(),
        Type::Named(name) => rust_type_name(name),
    }
}

pub(super) fn rust_type_name(name: &TypeName) -> String {
    name.code_name()
}

pub(super) fn returns_json(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Int
            | Type::Float
            | Type::Bool
            | Type::Object(_)
            | Type::Array(_)
            | Type::Map(_)
            | Type::Union(_)
            | Type::OneOf(_)
            | Type::Nullable(_)
            | Type::Any
            | Type::Named(_)
    )
}

pub(super) fn status_code_expr(status: u16) -> String {
    match status {
        200 => "StatusCode::OK".to_string(),
        201 => "StatusCode::CREATED".to_string(),
        202 => "StatusCode::ACCEPTED".to_string(),
        204 => "StatusCode::NO_CONTENT".to_string(),
        400 => "StatusCode::BAD_REQUEST".to_string(),
        401 => "StatusCode::UNAUTHORIZED".to_string(),
        403 => "StatusCode::FORBIDDEN".to_string(),
        404 => "StatusCode::NOT_FOUND".to_string(),
        409 => "StatusCode::CONFLICT".to_string(),
        422 => "StatusCode::UNPROCESSABLE_ENTITY".to_string(),
        500 => "StatusCode::INTERNAL_SERVER_ERROR".to_string(),
        other => format!("StatusCode::from_u16({}).unwrap()", other),
    }
}

pub(super) fn primary_response(route: &Route) -> &ResponseSpec {
    route
        .responses
        .iter()
        .find(|r| (200..300).contains(&r.status))
        .unwrap_or_else(|| &route.responses[0])
}

pub(super) fn handler_name(route: &Route) -> String {
    let method = match route.method {
        HttpMethod::Get => "get",
        HttpMethod::Post => "post",
        HttpMethod::Put => "put",
        HttpMethod::Patch => "patch",
        HttpMethod::Delete => "delete",
        HttpMethod::Options => "options",
        HttpMethod::Head => "head",
    };

    let mut name = String::new();
    name.push_str(method);
    name.push('_');
    let mut last_underscore = true;
    for ch in route.path.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch.to_ascii_lowercase());
            last_underscore = false;
        } else if ch == '/' || ch == '-' || ch == '_' || ch == '{' || ch == '}' {
            if !last_underscore {
                name.push('_');
                last_underscore = true;
            }
        }
    }
    while name.ends_with('_') {
        name.pop();
    }
    name
}

pub(super) fn method_func(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "get",
        HttpMethod::Post => "post",
        HttpMethod::Put => "put",
        HttpMethod::Patch => "patch",
        HttpMethod::Delete => "delete",
        HttpMethod::Options => "options",
        HttpMethod::Head => "head",
    }
}

pub(super) fn render_method_imports(service: &Service) -> String {
    let mut methods: std::collections::BTreeSet<&'static str> = std::collections::BTreeSet::new();
    for route in &service.http.routes {
        methods.insert(method_func(route.method.clone()));
    }
    if methods.is_empty() {
        methods.insert("get");
    }
    methods.into_iter().collect::<Vec<_>>().join(", ")
}

pub(super) fn needs_types_import(service: &Service) -> bool {
    for route in &service.http.routes {
        for field in route
            .input
            .path
            .iter()
            .chain(route.input.query.iter())
            .chain(route.input.body.iter())
            .chain(route.headers.iter())
        {
            if uses_named_type(&field.ty) {
                return true;
            }
        }
    }
    false
}

pub(super) fn uses_named_type(ty: &Type) -> bool {
    common_uses_named_type(ty)
}

pub(super) fn axum_path(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut name = String::new();
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == '}' {
                    break;
                }
                name.push(next);
            }
            if name.is_empty() {
                out.push_str("{}");
            } else {
                out.push(':');
                out.push_str(&name);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn to_snake_case(input: &str) -> String {
    let mut out = String::new();
    for (i, ch) in input.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn normalize_ident(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

pub(super) fn remote_call_type_name(name: &str) -> String {
    let mut out = String::new();
    let mut cap = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if cap {
                out.push(ch.to_ascii_uppercase());
                cap = false;
            } else {
                out.push(ch);
            }
        } else {
            cap = true;
        }
    }
    if out.is_empty() {
        "RemoteCall".to_string()
    } else {
        out
    }
}

pub(super) fn to_kebab_case(input: &str) -> String {
    let mut out = String::new();
    for (i, ch) in input.chars().enumerate() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() {
                if i != 0 {
                    out.push('-');
                }
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(ch.to_ascii_lowercase());
            }
        } else if ch == '_' || ch == '-' || ch == ' ' {
            if !out.ends_with('-') {
                out.push('-');
            }
        }
    }
    if out.is_empty() {
        "service-api".to_string()
    } else {
        out.trim_matches('-').to_string()
    }
}
