use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::ir::{AuthSpec, Service, effective_auth};

pub(super) fn generate_middleware(service: &Service, src_dir: &Path) -> Result<()> {
    let mut names = std::collections::BTreeSet::new();
    for name in &service.common.middleware {
        names.insert(name.clone());
    }
    for route in &service.http.routes {
        for name in &route.middleware {
            names.insert(name.clone());
        }
        if let Some(auth) = effective_auth(route.auth.as_ref(), service.common.auth.as_ref()) {
            names.insert(auth_middleware_name(auth));
        }
    }
    if names.is_empty() {
        return Ok(());
    }
    let mut out = String::new();
    out.push_str("use axum::{body::Body, http::{header::HeaderName, HeaderValue, Request, StatusCode}, middleware::Next, response::Response};\n\n");
    out.push_str("fn middleware_directive(req: &Request<Body>, name: &str) -> Option<String> {\n");
    out.push_str(
        "    let header = format!(\"x-nimesvc-middleware-{}\", name.replace('_', \"-\"));\n",
    );
    out.push_str("    req.headers().get(&header).and_then(|v| v.to_str().ok()).map(|v| v.trim().to_ascii_lowercase())\n");
    out.push_str("}\n\n");
    for name in names {
        let body = match name.as_str() {
            "auth_none" => "    Ok(next.run(req).await)\n".to_string(),
            "auth_bearer" => "    if req.headers().get(\"authorization\").is_none() {\n        return Err(StatusCode::UNAUTHORIZED);\n    }\n    Ok(next.run(req).await)\n".to_string(),
            "auth_api_key" => "    if req.headers().get(\"x-api-key\").is_none() {\n        return Err(StatusCode::UNAUTHORIZED);\n    }\n    Ok(next.run(req).await)\n".to_string(),
            _ => format!(
                "    if matches!(middleware_directive(&req, {name:?}).as_deref(), Some(\"block\" | \"deny\" | \"forbid\")) {{\n        return Err(StatusCode::FORBIDDEN);\n    }}\n    let mut response = next.run(req).await;\n    if let Ok(header_name) = HeaderName::from_bytes(b\"x-nimesvc-middleware-{header}\") {{\n        response.headers_mut().insert(header_name, HeaderValue::from_static(\"ok\"));\n    }}\n    Ok(response)\n",
                name = name,
                header = name.replace('_', "-")
            ),
        };
        out.push_str(&format!(
            "pub async fn {name}(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {{\n{body}}}\n\n",
            name = name,
            body = body
        ));
    }
    fs::write(src_dir.join("middleware.rs"), out)
        .with_context(|| "Failed to write middleware.rs")?;
    Ok(())
}

pub(super) fn auth_middleware_name(auth: &AuthSpec) -> String {
    match auth {
        AuthSpec::None => "auth_none".to_string(),
        AuthSpec::Bearer => "auth_bearer".to_string(),
        AuthSpec::ApiKey => "auth_api_key".to_string(),
    }
}
