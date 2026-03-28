use crate::generators::common::env::render_env_checks_rust;
use crate::ir::{EventsBroker, Service, Type};

use super::routes::route_middleware_chain;
use super::sockets::socket_handler_name;
use super::util::{
    axum_path, handler_name, method_func, needs_remote_calls, needs_types_import, primary_response,
    render_method_imports, returns_json,
};
use crate::generators::common::validation::{route_has_regex, route_has_validation};

pub(super) fn build_main_rs(service: &Service) -> String {
    let method_imports = render_method_imports(service);
    let mut imports = vec![
        format!(
            "use axum::{{routing::{{{}}}, Router, response::IntoResponse}};",
            method_imports
        ),
        "mod types;".to_string(),
    ];
    if !service.events.definitions.is_empty() {
        imports.push("mod events;".to_string());
    }
    if needs_types_import(service) {
        imports.push("#[allow(unused_imports)]\nuse types::*;".to_string());
    }
    if needs_remote_calls(service) {
        imports.push("mod remote_calls;".to_string());
        imports.push("use remote_calls::*;".to_string());
    }
    let mut needs_status = false;
    let mut needs_middleware = false;
    let needs_access_log = !service.http.routes.is_empty();
    let mut needs_json = false;
    let mut needs_path = false;
    let mut needs_query = false;
    let mut needs_headers = false;
    let mut needs_serde = false;
    let mut needs_rate_limit = false;
    let mut needs_regex = false;
    let needs_ws = !service.sockets.sockets.is_empty();
    let needs_cors = service.common.cors.is_some();
    let cors_needs_allow_origin = service
        .common
        .cors
        .as_ref()
        .map(|c| !c.allow_any)
        .unwrap_or(false);

    for route in &service.http.routes {
        let resp = primary_response(route);
        if resp.ty == Type::Void || resp.status != 200 {
            needs_status = true;
        }
        if returns_json(&resp.ty) || !route.input.body.is_empty() {
            needs_json = true;
        }
        if !route.input.path.is_empty() {
            needs_path = true;
            needs_serde = true;
        }
        if !route.input.query.is_empty() {
            needs_query = true;
            needs_serde = true;
        }
        if !route.headers.is_empty() || route.call.service_base.is_some() {
            needs_headers = true;
        }
        if route.headers.iter().any(|h| !h.optional) {
            needs_status = true;
        }
        if !route.input.body.is_empty() {
            needs_serde = true;
        }
        if !route.middleware.is_empty() || route.auth.is_some() {
            needs_middleware = true;
        }
        if route.rate_limit.is_some() || service.common.rate_limit.is_some() {
            needs_rate_limit = true;
            needs_status = true;
        }
        if route_has_validation(route) {
            needs_status = true;
            if route_has_regex(route) {
                needs_regex = true;
            }
        }
    }
    if !service.common.middleware.is_empty() {
        needs_middleware = true;
    }
    if !service.sockets.sockets.is_empty() {
        needs_headers = true;
        if service
            .sockets
            .sockets
            .iter()
            .any(|s| s.rate_limit.is_some())
        {
            needs_rate_limit = true;
        }
    }
    if !service.schema.types.is_empty() || !service.schema.enums.is_empty() {
        needs_serde = true;
    }

    if needs_status {
        imports.push("use axum::http::StatusCode;".to_string());
    }
    if needs_json {
        imports.push("use axum::Json;".to_string());
    }
    if needs_middleware || needs_access_log {
        imports.push("use axum::middleware::from_fn;".to_string());
    }
    if needs_path {
        imports.push("use axum::extract::Path;".to_string());
    }
    if needs_query {
        imports.push("use axum::extract::Query;".to_string());
    }
    if needs_headers {
        imports.push("use axum::http::HeaderMap;".to_string());
    }
    if needs_cors {
        if cors_needs_allow_origin {
            imports.push(
                "use tower_http::cors::{CorsLayer, Any, AllowOrigin, AllowMethods, AllowHeaders};"
                    .to_string(),
            );
        } else {
            imports.push(
                "use tower_http::cors::{CorsLayer, Any, AllowMethods, AllowHeaders};".to_string(),
            );
        }
        imports.push("use axum::http::{HeaderName, HeaderValue, Method};".to_string());
    }
    if needs_rate_limit {
        imports.push(
            "use axum::{body::Body, http::Request, middleware::Next, response::Response};"
                .to_string(),
        );
        imports.push("use std::sync::{Mutex, OnceLock};".to_string());
        imports.push("use std::time::{Duration, Instant};".to_string());
    } else if needs_access_log {
        imports.push(
            "use axum::{body::Body, http::Request, middleware::Next, response::Response};"
                .to_string(),
        );
        imports.push("use std::time::Instant;".to_string());
    }
    if !service.common.env.is_empty() {
        imports.push("use std::env;".to_string());
    }
    if needs_serde {
        imports.push("use serde::Deserialize;".to_string());
    }
    if needs_ws {
        imports.push("use axum::extract::ws::{WebSocketUpgrade, WebSocket, Message};".to_string());
        imports.push("use futures_util::{StreamExt, SinkExt};".to_string());
        imports.push("use tokio::sync::mpsc;".to_string());
        imports.push("use std::collections::HashMap;".to_string());
        imports.push("use std::sync::Arc;".to_string());
        imports.push("use std::sync::atomic::{AtomicU64, Ordering};".to_string());
        if !needs_rate_limit {
            imports.push("use std::sync::Mutex;".to_string());
            imports.push("use std::sync::OnceLock;".to_string());
        }
        if !needs_serde {
            imports.push("use serde::Deserialize;".to_string());
        }
        imports.push("use serde::Serialize;".to_string());
        imports.push("use serde_json::{Value, json};".to_string());
    }
    if needs_regex {
        imports.push("use regex::Regex;".to_string());
    }

    let mut routes_block = String::new();
    for route in &service.http.routes {
        let handler_name = handler_name(route);
        let method = method_func(route.method.clone());
        let mw_chain = route_middleware_chain(route, service);
        routes_block.push_str(&format!(
            "        .route(\"{}\", {}({}){})\n",
            axum_path(&route.path),
            method,
            handler_name,
            mw_chain
        ));
    }
    for socket in &service.sockets.sockets {
        routes_block.push_str(&format!(
            "        .route(\"{}\", get({}))\n",
            axum_path(&socket.path),
            socket_handler_name(&socket.name.code_name())
        ));
    }

    let mut handlers = String::new();
    handlers.push_str(&render_module_decls(service));
    if needs_middleware {
        handlers.push_str("mod middleware;\nuse middleware::*;\n");
    }
    handlers.push_str("\n");
    handlers.push_str(&super::routes::render_route_helpers(service));
    if needs_ws {
        handlers.push_str(&super::sockets::render_socket_helpers(service));
    }

    let address = service.common.address.as_deref().unwrap_or("127.0.0.1");
    let port = service.common.port.unwrap_or(3000);
    let env_checks = render_env_checks_rust(service);
    let cors_helpers = render_cors_helpers(service);
    let access_log_helper = render_access_log_helper(service);
    let needs_event_consumers = matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    ) && !service.events.subscribes.is_empty();
    let event_consumers = if needs_event_consumers {
        "    events::start_event_consumers().await;\n"
    } else {
        ""
    };
    format!(
        r#"{imports}

{cors_helpers}
{access_log_helper}

#[tokio::main]
async fn main() {{
{env_checks}
{event_consumers}
    let app = Router::new()
{routes_block}{global_layers}        ;

    let addr = "{address}:{port}";
    let listener = match tokio::net::TcpListener::bind(addr).await {{
        Ok(listener) => listener,
        Err(err) => {{
            eprintln!("Failed to bind to {{}}: {{}}", addr, err);
            return;
        }}
    }};
    if let Err(err) = axum::serve(listener, app).await {{
        eprintln!("Server error: {{}}", err);
    }}
}}

{handlers}
"#,
        imports = imports.join("\n"),
        routes_block = routes_block,
        global_layers = global_middleware_layers(service),
        handlers = handlers,
        address = address,
        port = port,
        cors_helpers = cors_helpers,
        access_log_helper = access_log_helper,
        env_checks = env_checks,
        event_consumers = event_consumers
    )
}

fn render_module_decls(service: &Service) -> String {
    let mut out = String::new();
    let mut seen = std::collections::HashSet::new();
    for module in &service.common.modules {
        let local_name = module.alias.as_deref().unwrap_or(&module.name);
        let key = format!(
            "{}|{}|{}",
            local_name,
            module.name,
            module.path.as_deref().unwrap_or("")
        );
        if !seen.insert(key) {
            continue;
        }
        if let Some(path) = &module.path {
            out.push_str(&format!(
                "mod {} {{\n    #![allow(dead_code, unused_imports)]\n    use crate::*;\n    use crate::types::*;\n    include!(\"{}\");\n}}\n",
                local_name, path
            ));
        } else {
            out.push_str(&format!(
                "#[allow(unused_imports)]\nuse {} as {};\n",
                module.name, local_name
            ));
        }
        out.push('\n');
    }
    out
}

// env checks are rendered via generators/common/env.rs

fn global_middleware_layers(service: &Service) -> String {
    let mut out = String::new();
    if let Some(cors) = &service.common.cors {
        if cors.allow_any {
            let methods = if cors.methods.is_empty() {
                "Any".to_string()
            } else {
                format!(
                    "AllowMethods::list(cors_methods(&[{}]))",
                    render_cors_str_list(&cors.methods)
                )
            };
            let headers = if cors.headers.is_empty() {
                "Any".to_string()
            } else {
                format!(
                    "AllowHeaders::list(cors_headers(&[{}]))",
                    render_cors_str_list(&cors.headers)
                )
            };
            out.push_str(&format!(
                "        .layer(CorsLayer::new().allow_origin(Any).allow_methods({}).allow_headers({}))\n",
                methods, headers
            ));
        } else {
            let origins = render_cors_str_list(&cors.origins);
            let methods = if cors.methods.is_empty() {
                "Any".to_string()
            } else {
                format!(
                    "AllowMethods::list(cors_methods(&[{}]))",
                    render_cors_str_list(&cors.methods)
                )
            };
            let headers = if cors.headers.is_empty() {
                "Any".to_string()
            } else {
                format!(
                    "AllowHeaders::list(cors_headers(&[{}]))",
                    render_cors_str_list(&cors.headers)
                )
            };
            out.push_str(&format!(
                "        .layer(CorsLayer::new().allow_origin(AllowOrigin::list(cors_origins(&[{}]))).allow_methods({}).allow_headers({}))\n",
                origins, methods, headers
            ));
        }
    }
    for name in &service.common.middleware {
        out.push_str(&format!("        .layer(from_fn({}))\n", name));
    }
    if !service.http.routes.is_empty() {
        out.push_str("        .layer(from_fn(access_log))\n");
    }
    out
}

fn render_access_log_helper(service: &Service) -> String {
    if service.http.routes.is_empty() {
        return String::new();
    }
    let service_name = &service.name;
    format!(
        r#"
async fn access_log(req: Request<Body>, next: Next) -> Response {{
    let method = req.method().clone();
    let path = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    let started = Instant::now();
    let response = next.run(req).await;
    let status = response.status().as_u16();
    let elapsed = started.elapsed().as_millis();
    eprintln!("[{service_name}] {{}} {{}} -> {{}} ({{}}ms)", method, path, status, elapsed);
    response
}}
"#
    )
}

fn render_cors_helpers(service: &Service) -> String {
    if service.common.cors.is_none() {
        return String::new();
    }
    r#"
fn cors_methods(values: &[&str]) -> Vec<Method> {
    values
        .iter()
        .filter_map(|value| Method::from_bytes(value.as_bytes()).ok())
        .collect()
}

fn cors_headers(values: &[&str]) -> Vec<HeaderName> {
    values
        .iter()
        .filter_map(|value| HeaderName::from_bytes(value.as_bytes()).ok())
        .collect()
}

fn cors_origins(values: &[&str]) -> Vec<HeaderValue> {
    values
        .iter()
        .filter_map(|value| HeaderValue::from_str(value).ok())
        .collect()
}
"#
    .to_string()
}

fn render_cors_str_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}
