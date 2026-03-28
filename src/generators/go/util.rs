use crate::ir::{
    AuthSpec, EventsBroker, InputRef, InputSource, RateLimit, ResponseSpec, Route, Service, Type,
    TypeName, UseScope,
};

pub(super) const GO_FN_PREFIX: &str = "Ns";

pub(super) fn build_go_mod(service: &Service, module_name: &str) -> String {
    let mut requires = Vec::new();
    requires.push((
        "github.com/go-chi/chi/v5".to_string(),
        "v5.0.10".to_string(),
    ));
    if !service.sockets.sockets.is_empty() {
        requires.push((
            "github.com/gorilla/websocket".to_string(),
            "v1.5.1".to_string(),
        ));
    }
    if matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    ) {
        requires.push((
            "github.com/redis/go-redis/v9".to_string(),
            "v9.5.1".to_string(),
        ));
    }
    for module in &service.common.modules {
        if module.path.is_some() {
            continue;
        }
        if let Some(ver) = &module.version {
            requires.push((module.name.clone(), ver.clone()));
        }
    }
    let mut out = String::new();
    out.push_str(&format!("module {}\n\ngo 1.20\n\n", module_name));
    if !requires.is_empty() {
        out.push_str("require (\n");
        for (name, ver) in requires {
            out.push_str(&format!("    {} {}\n", name, ver));
        }
        out.push_str(")\n");
    }
    out
}

pub(crate) fn go_type(ty: &Type, use_types_pkg: bool) -> String {
    match ty {
        Type::String => "string".to_string(),
        Type::Int => "int64".to_string(),
        Type::Float => "float64".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Array(inner) => format!("[]{}", go_type(inner, use_types_pkg)),
        Type::Map(inner) => format!("map[string]{}", go_type(inner, use_types_pkg)),
        Type::Union(_) | Type::OneOf(_) => "any".to_string(),
        Type::Nullable(inner) => format!("*{}", go_type(inner, use_types_pkg)),
        Type::Object(_) => "map[string]any".to_string(),
        Type::Named(name) => go_type_name(name, use_types_pkg),
        Type::Any => "any".to_string(),
        Type::Void => "struct{}".to_string(),
    }
}

pub(crate) fn wrap_optional(go_ty: &str) -> String {
    if go_ty.starts_with('*') {
        go_ty.to_string()
    } else {
        format!("*{}", go_ty)
    }
}

pub(super) fn go_wrapper_name(raw: &str) -> String {
    format!("{}{}", GO_FN_PREFIX, to_go_func_name(raw))
}

pub(super) fn go_type_name(name: &TypeName, use_types_pkg: bool) -> String {
    let base = name.code_name();
    if use_types_pkg {
        format!("types.{}", base)
    } else {
        base
    }
}

pub(super) fn go_call_name(raw: &str) -> String {
    if raw.starts_with(GO_FN_PREFIX) {
        to_go_func_name(raw)
    } else {
        go_wrapper_name(raw)
    }
}

pub(crate) fn type_uses_types_pkg(ty: &Type) -> bool {
    match ty {
        Type::Named(_) => true,
        Type::Array(inner) => type_uses_types_pkg(inner),
        Type::Map(inner) => type_uses_types_pkg(inner),
        Type::Union(types) | Type::OneOf(types) => types.iter().any(type_uses_types_pkg),
        Type::Nullable(inner) => type_uses_types_pkg(inner),
        Type::Object(_) => false,
        _ => false,
    }
}

pub(super) fn render_module_imports(
    service: &Service,
    module_name: &str,
    scope: UseScope,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
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
        let key = format!("{}|{}|{}", local, module.name, module.path.is_some());
        if !seen.insert(key) {
            continue;
        }
        if module.path.is_some() {
            let import_path = format!("{}/modules/{}", module_name, local);
            out.push(format!("{} \"{}\"", local, import_path));
        } else {
            out.push(format!("{} \"{}\"", local, module.name));
        }
    }
    out
}

pub(super) fn module_import_alias(raw: &str) -> String {
    raw.to_string()
}

pub(super) fn handler_name(route: &Route) -> String {
    let mut base = String::new();
    base.push_str(match route.method {
        crate::ir::HttpMethod::Get => "get",
        crate::ir::HttpMethod::Post => "post",
        crate::ir::HttpMethod::Put => "put",
        crate::ir::HttpMethod::Patch => "patch",
        crate::ir::HttpMethod::Delete => "delete",
        crate::ir::HttpMethod::Options => "options",
        crate::ir::HttpMethod::Head => "head",
    });
    base.push_str(&sanitize_name(&route.path));
    base
}

pub(super) fn sanitize_name(path: &str) -> String {
    let mut out = String::new();
    for ch in path.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out
}

pub(super) fn path_struct_name(route: &Route) -> String {
    format!("{}Path", handler_name(route).to_uppercase())
}

pub(super) fn query_struct_name(route: &Route) -> String {
    format!("{}Query", handler_name(route).to_uppercase())
}

pub(super) fn body_struct_name(route: &Route) -> String {
    format!("{}Body", handler_name(route).to_uppercase())
}

pub(super) fn headers_struct_name(route: &Route) -> String {
    format!("{}Headers", handler_name(route).to_uppercase())
}

pub(super) fn to_go_func_name(raw: &str) -> String {
    go_field_name(raw)
}

pub(crate) fn go_field_name(raw: &str) -> String {
    let mut out = String::new();
    let mut cap = true;
    for ch in raw.chars() {
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
        "Field".to_string()
    } else {
        out
    }
}

pub(super) fn auth_middleware_name(auth: &AuthSpec) -> String {
    match auth {
        AuthSpec::None => "auth_none".to_string(),
        AuthSpec::Bearer => "auth_bearer".to_string(),
        AuthSpec::ApiKey => "auth_api_key".to_string(),
    }
}

pub(super) fn middleware_name(raw: &str) -> String {
    raw.to_string()
}

pub(super) fn http_method(route: &Route) -> &'static str {
    match route.method {
        crate::ir::HttpMethod::Get => "GET",
        crate::ir::HttpMethod::Post => "POST",
        crate::ir::HttpMethod::Put => "PUT",
        crate::ir::HttpMethod::Patch => "PATCH",
        crate::ir::HttpMethod::Delete => "DELETE",
        crate::ir::HttpMethod::Options => "OPTIONS",
        crate::ir::HttpMethod::Head => "HEAD",
    }
}

pub(super) fn primary_response(route: &Route) -> &ResponseSpec {
    route
        .responses
        .iter()
        .find(|r| (200..300).contains(&r.status))
        .unwrap_or_else(|| &route.responses[0])
}

pub(super) fn needs_json(service: &Service) -> bool {
    for route in &service.http.routes {
        if !route.input.body.is_empty() {
            return true;
        }
        if route
            .input
            .path
            .iter()
            .chain(route.input.query.iter())
            .chain(route.headers.iter())
            .any(|f| !matches!(f.ty, Type::String | Type::Int | Type::Float | Type::Bool))
        {
            return true;
        }
        let resp = primary_response(route);
        if resp.ty != Type::String && resp.ty != Type::Void {
            return true;
        }
    }
    false
}

pub(super) fn needs_strconv(service: &Service) -> bool {
    let mut fields = Vec::new();
    for route in &service.http.routes {
        fields.extend(route.input.path.iter());
        fields.extend(route.input.query.iter());
        fields.extend(route.headers.iter());
    }
    fields
        .iter()
        .any(|f| matches!(f.ty, Type::Int | Type::Float | Type::Bool))
}

pub(super) fn needs_types_in_main(service: &Service) -> bool {
    for route in &service.http.routes {
        for field in route
            .input
            .path
            .iter()
            .chain(route.input.query.iter())
            .chain(route.input.body.iter())
            .chain(route.headers.iter())
        {
            if type_uses_types_pkg(&field.ty) {
                return true;
            }
        }
    }
    false
}

pub(super) fn needs_remote_calls(service: &Service) -> bool {
    service
        .http
        .routes
        .iter()
        .any(|r| r.call.service_base.is_some())
}

pub(super) fn needs_rate_limit(service: &Service) -> bool {
    service
        .http
        .routes
        .iter()
        .any(|r| r.rate_limit.is_some() || service.common.rate_limit.is_some())
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

pub(super) fn rate_limit_name(route: &Route) -> String {
    format!("rate_limit_{}", handler_name(route))
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

pub(super) fn remote_call_fn_name(route: &Route) -> String {
    let service = route.call.service.as_deref().unwrap_or("service");
    format!(
        "call_{}_{}_{}",
        normalize_ident(service),
        normalize_ident(&route.call.module),
        normalize_ident(&route.call.function)
    )
}

pub(super) fn resolve_input_ref_type(
    service: &Service,
    route: &Route,
    input: &InputRef,
) -> ResolvedType {
    let fields: &[crate::ir::Field] = match input.source {
        InputSource::Path => &route.input.path,
        InputSource::Query => &route.input.query,
        InputSource::Body => &route.input.body,
        InputSource::Headers => &route.headers,
        InputSource::Input => &[],
    };
    if input.path.is_empty() {
        return ResolvedType {
            ty: Type::Object(fields.to_vec()),
            optional: false,
        };
    }
    let current = fields
        .iter()
        .find(|f| f.name == input.path[0])
        .cloned()
        .unwrap_or(crate::ir::Field {
            name: input.path[0].clone(),
            ty: Type::Any,
            optional: false,
            validation: None,
        });
    let mut optional = current.optional;
    let mut ty = current.ty.clone();
    for segment in input.path.iter().skip(1) {
        let resolved = resolve_field_in_type(service, &ty, segment);
        optional = optional || resolved.optional;
        ty = resolved.ty;
    }
    ResolvedType { ty, optional }
}

fn resolve_field_in_type(service: &Service, ty: &Type, name: &str) -> ResolvedType {
    let fields = match ty {
        Type::Object(fields) => fields.clone(),
        Type::Named(name) => service
            .schema
            .types
            .iter()
            .find(|t| t.name == *name)
            .map(|t| t.fields.clone())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let field = fields
        .iter()
        .find(|f| f.name == name)
        .cloned()
        .unwrap_or(crate::ir::Field {
            name: name.to_string(),
            ty: Type::Any,
            optional: false,
            validation: None,
        });
    ResolvedType {
        ty: field.ty,
        optional: field.optional,
    }
}

pub(super) struct ResolvedType {
    pub(super) ty: Type,
    pub(super) optional: bool,
}
