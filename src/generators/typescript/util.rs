use crate::ir::{
    AuthSpec, HttpMethod, RateLimit, ResponseSpec, Route, Service, Type, TypeName, UseScope,
    effective_auth,
};

pub(super) fn primary_response(route: &Route) -> &ResponseSpec {
    route
        .responses
        .iter()
        .find(|r| (200..300).contains(&r.status))
        .unwrap_or_else(|| &route.responses[0])
}

pub(super) fn ts_type(ty: &Type) -> String {
    ts_type_internal(ty, false)
}

pub(super) fn ts_client_type(ty: &Type) -> String {
    ts_type_internal(ty, true)
}

fn ts_type_internal(ty: &Type, use_types: bool) -> String {
    match ty {
        Type::String => "string".to_string(),
        Type::Int | Type::Float => "number".to_string(),
        Type::Bool => "boolean".to_string(),
        Type::Object(fields) => {
            if fields.is_empty() {
                "Record<string, unknown>".to_string()
            } else {
                let mut out = String::from("{ ");
                let mut first = true;
                for field in fields {
                    if !first {
                        out.push_str("; ");
                    }
                    first = false;
                    if field.optional {
                        out.push_str(&format!(
                            "{}?: {}",
                            field.name,
                            ts_type_internal(&field.ty, use_types)
                        ));
                    } else {
                        out.push_str(&format!(
                            "{}: {}",
                            field.name,
                            ts_type_internal(&field.ty, use_types)
                        ));
                    }
                }
                out.push_str(" }");
                out
            }
        }
        Type::Array(inner) => format!("Array<{}>", ts_type_internal(inner, use_types)),
        Type::Map(inner) => format!("Record<string, {}>", ts_type_internal(inner, use_types)),
        Type::Union(types) | Type::OneOf(types) => {
            let mut parts = Vec::new();
            for ty in types {
                parts.push(ts_type_internal(ty, use_types));
            }
            parts.join(" | ")
        }
        Type::Nullable(inner) => {
            let base = ts_type_internal(inner, use_types);
            let wrapped = if base.contains(" | ") {
                format!("({})", base)
            } else {
                base
            };
            format!("{} | null", wrapped)
        }
        Type::Any => "any".to_string(),
        Type::Void => "void".to_string(),
        Type::Named(name) => {
            if use_types {
                format!("Types.{}", ts_type_name(name))
            } else {
                ts_type_name(name)
            }
        }
    }
}

pub(super) fn method_name(method: HttpMethod) -> &'static str {
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

pub(super) fn express_path(path: &str) -> String {
    path.replace('{', ":").replace('}', "")
}

pub(crate) fn render_module_imports(service: &Service, scope: UseScope) -> Vec<String> {
    let mut out = Vec::new();
    for module in &service.common.modules {
        if scope == UseScope::Compile && !matches!(module.scope, UseScope::Compile | UseScope::Both)
        {
            continue;
        }
        if scope == UseScope::Runtime && !matches!(module.scope, UseScope::Runtime | UseScope::Both)
        {
            continue;
        }
        let local = module.alias.as_deref().unwrap_or(&module.name);
        if let Some(path) = &module.path {
            let import_path = ts_import_path(path);
            out.push(format!("import * as {} from '{}';", local, import_path));
        } else {
            out.push(format!("import * as {} from '{}';", local, module.name));
        }
    }
    out
}

pub(super) fn ts_import_path(path: &str) -> String {
    let mut p = path.replace('\\', "/");
    if p.ends_with(".ts") {
        p = p.trim_end_matches(".ts").to_string();
    } else if p.ends_with(".js") {
        p = p.trim_end_matches(".js").to_string();
    }
    if !p.starts_with('.') {
        p = format!("./{}", p);
    }
    p
}

pub(super) fn ts_type_name(name: &TypeName) -> String {
    name.code_name()
}

pub(super) fn handler_struct_base(route: &Route) -> String {
    let mut base = String::new();
    base.push_str(match route.method {
        HttpMethod::Get => "Get",
        HttpMethod::Post => "Post",
        HttpMethod::Put => "Put",
        HttpMethod::Patch => "Patch",
        HttpMethod::Delete => "Delete",
        HttpMethod::Options => "Options",
        HttpMethod::Head => "Head",
    });
    let mut segment_start = true;
    for ch in route.path.chars() {
        if ch.is_ascii_alphanumeric() {
            if segment_start {
                base.push(ch.to_ascii_uppercase());
                segment_start = false;
            } else {
                base.push(ch.to_ascii_lowercase());
            }
        } else if ch == '/' || ch == '-' || ch == '_' || ch == '{' || ch == '}' {
            segment_start = true;
        }
    }
    base
}

pub(super) fn path_struct_name(route: &Route) -> String {
    format!("{}Path", handler_struct_base(route))
}

pub(super) fn query_struct_name(route: &Route) -> String {
    format!("{}Query", handler_struct_base(route))
}

pub(super) fn body_struct_name(route: &Route) -> String {
    format!("{}Body", handler_struct_base(route))
}

pub(super) fn headers_struct_name(route: &Route) -> String {
    format!("{}Headers", handler_struct_base(route))
}

pub(super) fn needs_middleware(service: &Service) -> bool {
    if !service.common.middleware.is_empty() {
        return true;
    }
    service.http.routes.iter().any(|r| {
        !r.middleware.is_empty()
            || effective_auth(r.auth.as_ref(), service.common.auth.as_ref()).is_some()
    })
}

pub(super) fn auth_middleware_name(auth: &AuthSpec) -> String {
    match auth {
        AuthSpec::None => "auth_none".to_string(),
        AuthSpec::Bearer => "auth_bearer".to_string(),
        AuthSpec::ApiKey => "auth_api_key".to_string(),
    }
}

pub(super) fn needs_remote_calls(service: &Service) -> bool {
    service
        .http
        .routes
        .iter()
        .any(|r| r.call.service_base.is_some())
}

pub(super) fn rate_limit_name(route: &Route) -> String {
    format!("rate_limit_{}", handler_struct_base(route).to_lowercase())
}

pub(super) fn effective_rate_limit<'a>(
    route: &'a Route,
    service: &'a Service,
) -> Option<&'a RateLimit> {
    route
        .rate_limit
        .as_ref()
        .or(service.common.rate_limit.as_ref())
}

pub(super) fn remote_call_fn_name(route: &Route) -> String {
    let service = route.call.service.as_deref().unwrap_or("service");
    format!(
        "call_{}_{}_{}",
        normalize_ident(service),
        normalize_ident(&route.call.module),
        normalize_ident(&route.call.function)
    )
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

fn normalize_ident(raw: &str) -> String {
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
