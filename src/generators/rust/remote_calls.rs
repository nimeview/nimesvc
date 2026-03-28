use crate::ir::{InputRef, InputSource, Route, Service, Type};

use super::util::{normalize_ident, primary_response, remote_call_type_name, rust_type};

pub(super) fn build_remote_calls_rs(service: &Service) -> String {
    let calls = collect_remote_calls(service);
    if calls.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("#![allow(dead_code)]\n");
    out.push_str("use axum::http::{HeaderMap, StatusCode};\n");
    out.push_str("use reqwest::Client;\n");
    out.push_str("use serde_json::json;\n");
    out.push_str("#[allow(unused_imports)]\nuse crate::types::*;\n\n");

    for call in calls {
        let type_name = remote_call_type_name(&call.name);
        out.push_str(&format!("pub struct {name}Request {{\n", name = type_name));
        for arg in &call.args {
            let ty = rust_type(&arg.ty);
            if arg.optional {
                out.push_str(&format!("    pub {}: Option<{}>,\n", arg.name, ty));
            } else {
                out.push_str(&format!("    pub {}: {},\n", arg.name, ty));
            }
        }
        out.push_str("}\n");
        out.push_str(&format!(
            "pub type {name}Response = {ty};\n\n",
            name = type_name,
            ty = rust_type(&call.response)
        ));

        let args_sig = call
            .args
            .iter()
            .map(|arg| {
                let ty = rust_type(&arg.ty);
                if arg.optional {
                    format!("{}: Option<{}>", arg.name, ty)
                } else {
                    format!("{}: {}", arg.name, ty)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let payload = call
            .args
            .iter()
            .map(|arg| format!("\"{}\": {}", arg.name, arg.name))
            .collect::<Vec<_>>()
            .join(", ");
        let ret_ty = rust_type(&call.response);
        let parse_expr = match call.response {
            Type::Void => "let _ = resp.text().await; Ok(())".to_string(),
            Type::String => {
                "let text = resp.text().await.unwrap_or_default();\n    match serde_json::from_str::<String>(&text) {\n        Ok(v) => Ok(v),\n        Err(_) => Ok(text),\n    }".to_string()
            }
            _ => format!(
                "match resp.json::<{ty}>().await {{\n        Ok(v) => Ok(v),\n        Err(_) => Err(StatusCode::BAD_GATEWAY),\n    }}",
                ty = ret_ty
            ),
        };
        out.push_str(&format!(
            r#"pub async fn {name}({args}, headers: &HeaderMap) -> Result<{ret}, StatusCode> {{
    let client = Client::new();
    let url = format!("{base}/__call/{module}/{func}");
    let payload = json!({{{payload}}});
    let mut req = client.post(url).json(&payload);
    if let Some(value) = headers.get("authorization") {{
        if let Ok(v) = value.to_str() {{
            req = req.header("authorization", v);
        }}
    }}
    for (name, value) in headers.iter() {{
        let key = name.as_str();
        if key.starts_with("x-") {{
            if let Ok(v) = value.to_str() {{
                req = req.header(key, v);
            }}
        }}
    }}
    let resp = match req.send().await {{
        Ok(r) => r,
        Err(_) => return Err(StatusCode::BAD_GATEWAY),
    }};
    if !resp.status().is_success() {{
        return Err(StatusCode::BAD_GATEWAY);
    }}
    {parse_expr}
}}

"#,
            name = call.name,
            args = args_sig,
            ret = ret_ty,
            base = call.base,
            module = call.module,
            func = call.function,
            payload = payload,
            parse_expr = parse_expr
        ));
    }
    out
}

struct RemoteCallSpec {
    name: String,
    base: String,
    module: String,
    function: String,
    args: Vec<RemoteArgSpec>,
    response: Type,
}

struct RemoteArgSpec {
    name: String,
    ty: Type,
    optional: bool,
}

fn collect_remote_calls(service: &Service) -> Vec<RemoteCallSpec> {
    let mut by_key: std::collections::BTreeMap<String, RemoteCallSpec> =
        std::collections::BTreeMap::new();
    for route in &service.http.routes {
        let Some(base) = &route.call.service_base else {
            continue;
        };
        let key = format!(
            "{}::{}::{}::{}",
            route.call.service.as_deref().unwrap_or("service"),
            route.call.module,
            route.call.function,
            base
        );
        if by_key.contains_key(&key) {
            continue;
        }
        let mut args = Vec::new();
        for (idx, arg) in route.call.args.iter().enumerate() {
            let name = arg.name.clone().unwrap_or_else(|| format!("arg{}", idx));
            let resolved = resolve_input_ref_type(service, route, &arg.value);
            args.push(RemoteArgSpec {
                name,
                ty: resolved.ty,
                optional: resolved.optional,
            });
        }
        let resp = primary_response(route);
        let spec = RemoteCallSpec {
            name: remote_call_fn_name(route),
            base: base.clone(),
            module: route.call.module.clone(),
            function: route.call.function.clone(),
            args,
            response: resp.ty.clone(),
        };
        by_key.insert(key, spec);
    }
    by_key.into_values().collect()
}

struct ResolvedType {
    ty: Type,
    optional: bool,
}

fn resolve_input_ref_type(service: &Service, route: &Route, input: &InputRef) -> ResolvedType {
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

pub(super) fn remote_call_fn_name(route: &Route) -> String {
    let service = route.call.service.as_deref().unwrap_or("service");
    format!(
        "call_{}_{}_{}",
        normalize_ident(service),
        normalize_ident(&route.call.module),
        normalize_ident(&route.call.function)
    )
}
