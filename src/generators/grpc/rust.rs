use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::generators::rpc;
use crate::generators::rust::events::build_events_rs as build_events_rs_shared;
use crate::ir::{
    AuthSpec, CallArg, EventsBroker, InputRef, InputSource, RateLimit, RpcDef, Service, Type,
};

pub(super) fn generate(service: &Service, out_dir: &Path) -> Result<()> {
    let src_dir = out_dir.join("src");
    fs::create_dir_all(&src_dir)
        .with_context(|| format!("Failed to create '{}'", src_dir.display()))?;

    let crate_name = format!("{}-grpc", service.name.to_lowercase());
    let cargo_toml = build_cargo_toml(service, &crate_name);
    fs::write(out_dir.join("Cargo.toml"), cargo_toml)
        .with_context(|| "Failed to write Cargo.toml")?;

    let proto = rpc::build_proto(service);
    fs::write(out_dir.join("rpc.proto"), proto).with_context(|| "Failed to write rpc.proto")?;

    let build_rs = r#"
fn main() {
    tonic_build::configure()
        .build_server(true)
        .compile(&["rpc.proto"], &["."])
        .unwrap();
}
"#;
    fs::write(out_dir.join("build.rs"), build_rs).with_context(|| "Failed to write build.rs")?;

    let types_rs = build_types_rs(service);
    fs::write(src_dir.join("types.rs"), types_rs)
        .with_context(|| "Failed to write src/types.rs")?;

    let events_rs = build_events_rs_shared(service);
    if !events_rs.is_empty() {
        fs::write(src_dir.join("events.rs"), events_rs)
            .with_context(|| "Failed to write src/events.rs")?;
    }

    let main_rs = build_main_rs(service);
    fs::write(src_dir.join("main.rs"), main_rs).with_context(|| "Failed to write src/main.rs")?;

    let middleware_rs = build_middleware_rs(service);
    if !middleware_rs.is_empty() {
        fs::write(src_dir.join("middleware.rs"), middleware_rs)
            .with_context(|| "Failed to write src/middleware.rs")?;
    }

    Ok(())
}

fn build_cargo_toml(service: &Service, crate_name: &str) -> String {
    let mut extra = String::new();
    if !service.events.definitions.is_empty() {
        extra.push_str("once_cell = \"1.19\"\n");
    }
    if matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    ) {
        extra.push_str("redis = { version = \"0.25\", features = [\"tokio-comp\"] }\n");
    }
    format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = {{ version = "1", features = ["full"] }}
tonic = "0.11"
prost = "0.12"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1.0"
{extra}

[build-dependencies]
tonic-build = "0.11"
"#,
        crate_name = crate_name,
        extra = extra
    )
}

fn build_main_rs(service: &Service) -> String {
    let service_name = &service.name;
    let service_mod = format!("nimesvc.{}", service.name.to_lowercase());
    let handler_name = format!("{}Service", service.name);
    let server_mod = format!("{}_server", to_snake_case(&service.name));
    let module_decls = render_module_decls(service);
    let events_mod = if service.events.definitions.is_empty() {
        String::new()
    } else {
        "mod events;\n".to_string()
    };
    let needs_headers = service.rpc.methods.iter().any(|r| !r.headers.is_empty());
    let needs_rate_limit = service
        .rpc
        .methods
        .iter()
        .any(|r| effective_rate_limit(r, service).is_some());
    let needs_auth = service
        .rpc
        .methods
        .iter()
        .any(|r| effective_auth(r, service).is_some());
    let needs_middleware = service
        .rpc
        .methods
        .iter()
        .any(|r| !r.middleware.is_empty() || !service.common.middleware.is_empty());
    let needs_meta_helpers = needs_headers || needs_auth || needs_middleware;
    let needs_access_log = !service.rpc.methods.is_empty();

    let mut methods = String::new();
    for rpc in &service.rpc.methods {
        methods.push_str(&render_rpc_method(rpc, service));
        methods.push('\n');
    }

    let mut helpers = String::new();
    if needs_headers {
        for rpc in &service.rpc.methods {
            helpers.push_str(&render_rpc_header_struct(rpc));
        }
    }
    if needs_meta_helpers {
        helpers.push_str(
            r#"
fn meta_str(meta: &tonic::metadata::MetadataMap, name: &str) -> Option<String> {
    meta.get(name).and_then(|v| v.to_str().ok()).map(|s| s.to_string())
}

fn meta_parse<T: std::str::FromStr>(meta: &tonic::metadata::MetadataMap, name: &str) -> Option<T> {
    meta.get(name).and_then(|v| v.to_str().ok()).and_then(|s| s.parse::<T>().ok())
}
"#,
        );
    }
    if needs_rate_limit {
        helpers.push_str(&render_rate_limit_helpers(service));
    }

    let (addr, port) = grpc_addr_port(service);
    let tls_block = render_tls_block(service);
    let max_msg_block = render_max_message_block(service);
    let middleware_mod = if needs_middleware {
        "mod middleware;\nuse middleware::*;\n"
    } else {
        ""
    };
    let extra_imports = build_extra_imports(
        needs_headers,
        needs_rate_limit,
        needs_meta_helpers,
        service,
        !service.sockets.sockets.is_empty(),
    );
    let access_log_import = if needs_access_log {
        "use std::time::Instant;\n"
    } else {
        ""
    };
    let socket_stubs = render_socket_stub_types(service);
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
        r#"use tonic::{{transport::Server, Request, Response, Status}};
{access_log_import}{extra_imports}

pub mod rpc {{
    tonic::include_proto!("{service_mod}");
}}

use rpc::{server_mod}::{{{service_name}, {service_name}Server}};
mod types;
{events_mod}
use types::*;

{module_decls}
{socket_stubs}
{middleware_mod}

#[derive(Default)]
struct {handler_name};

#[tonic::async_trait]
impl {service_name} for {handler_name} {{
{methods}
}}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {{
    let addr = "{addr}:{port}".parse()?;
    let handler = {handler_name}::default();
    let mut service = {service_name}Server::new(handler);
{event_consumers}
{max_msg_block}    let mut builder = Server::builder();
{tls_block}    builder
        .add_service(service)
        .serve(addr)
        .await?;
    Ok(())
}}
{helpers}
"#,
        service_mod = service_mod,
        service_name = service_name,
        server_mod = server_mod,
        handler_name = handler_name,
        methods = methods,
        module_decls = module_decls,
        events_mod = events_mod,
        socket_stubs = socket_stubs,
        middleware_mod = middleware_mod,
        helpers = helpers,
        access_log_import = access_log_import,
        extra_imports = extra_imports,
        addr = addr,
        port = port,
        tls_block = tls_block,
        max_msg_block = max_msg_block,
        event_consumers = event_consumers,
    )
}

fn render_rpc_method(rpc: &RpcDef, service: &Service) -> String {
    let method_name = to_snake_case(&rpc.code_name());
    let req = format!("{}Request", rpc.code_name());
    let resp_ty = rpc_response_type(rpc);
    let call_expr = render_call_expr(rpc);
    let response_expr = render_response_expr(rpc);
    let auth_check = render_auth_check(rpc, service);
    let middleware_calls = render_middleware_calls(rpc, service);
    let rate_limit = render_rate_limit_check(rpc, service);
    let headers_init = render_headers_init(rpc);
    let meta_init = if needs_metadata(rpc, service) {
        "        let metadata = request.metadata();\n"
    } else {
        ""
    };

    format!(
        "    async fn {method_name}(&self, request: Request<rpc::{req}>) -> Result<Response<rpc::{resp_ty}>, Status> {{\n        let started = Instant::now();\n        let result: Result<Response<rpc::{resp_ty}>, Status> = async {{\n{meta_init}{auth_check}{headers_init}{middleware_calls}{rate_limit}            let input = request.into_inner();\n            let result = {call_expr};\n            let response = {response_expr};\n            Ok(Response::new(response))\n        }}.await;\n        let status = match &result {{\n            Ok(_) => \"OK\".to_string(),\n            Err(err) => err.code().to_string(),\n        }};\n        eprintln!(\"[{service_name}] gRPC {rpc_name} -> {{}} ({{}}ms)\", status, started.elapsed().as_millis());\n        result\n    }}",
        method_name = method_name,
        req = req,
        resp_ty = resp_ty,
        call_expr = call_expr,
        response_expr = response_expr,
        auth_check = auth_check,
        middleware_calls = middleware_calls,
        rate_limit = rate_limit,
        headers_init = headers_init,
        meta_init = meta_init,
        service_name = service.name,
        rpc_name = rpc.code_name(),
    )
}

fn rpc_response_type(rpc: &RpcDef) -> String {
    match &rpc.output {
        Type::Void => "google::protobuf::Empty".to_string(),
        Type::Object(_) => format!("{}Response", rpc.code_name()),
        Type::Any | Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) => {
            "google::protobuf::Struct".to_string()
        }
        Type::Named(name) => name.code_name(),
        _ => format!("{}Response", rpc.code_name()),
    }
}

fn render_response_expr(rpc: &RpcDef) -> String {
    match &rpc.output {
        Type::Void => "rpc::google::protobuf::Empty {}".to_string(),
        Type::Object(_) => "result".to_string(),
        Type::Any | Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) => {
            "result".to_string()
        }
        Type::Named(_) => "result".to_string(),
        Type::Array(_) | Type::String | Type::Int | Type::Float | Type::Bool => {
            format!("rpc::{}Response {{ value: result }}", rpc.code_name())
        }
    }
}

fn render_call_expr(rpc: &RpcDef) -> String {
    let module = &rpc.call.module;
    let func = &rpc.call.function;
    let args = render_call_args(&rpc.call.args);
    if rpc.call.is_async {
        format!("{module}::{func}({args}).await")
    } else {
        format!("{module}::{func}({args})")
    }
}

fn render_call_args(args: &[CallArg]) -> String {
    let mut out = Vec::new();
    for arg in args {
        out.push(render_input_ref(&arg.value));
    }
    out.join(", ")
}

fn render_input_ref(input: &InputRef) -> String {
    if input.source != InputSource::Input {
        let base = match input.source {
            InputSource::Headers => "headers",
            _ => "input",
        };
        if input.path.is_empty() {
            return base.to_string();
        }
        return format!("{}.{}", base, input.path.join("."));
    }
    if input.path.is_empty() {
        return "input".to_string();
    }
    format!("input.{}", input.path.join("."))
}

fn build_types_rs(service: &Service) -> String {
    let mut out = String::new();
    out.push_str("#![allow(dead_code)]\n");
    out.push_str("use crate::rpc;\n\n");
    for en in &service.schema.enums {
        out.push_str(&format!(
            "pub type {} = rpc::{};\n",
            en.name.code_name(),
            en.name.code_name()
        ));
    }
    for ty in &service.schema.types {
        out.push_str(&format!(
            "pub type {} = rpc::{};\n",
            ty.name.code_name(),
            ty.name.code_name()
        ));
    }
    for rpc in &service.rpc.methods {
        out.push_str(&format!(
            "pub type {}Request = rpc::{}Request;\n",
            rpc.code_name(),
            rpc.code_name()
        ));
        let resp_ty = rpc_response_type(rpc);
        out.push_str(&format!(
            "pub type {}Response = rpc::{};\n",
            rpc.code_name(),
            resp_ty
        ));
    }
    out
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
                "mod {} {{\n    #[allow(unused_imports)]\n    use crate::*;\n    use crate::types::*;\n    include!(\"{}\");\n}}\n",
                local_name, path
            ));
        } else {
            out.push_str(&format!("use {} as {};\n", module.name, local_name));
        }
        out.push('\n');
    }
    out
}

fn build_extra_imports(
    needs_headers: bool,
    needs_rate_limit: bool,
    needs_meta: bool,
    service: &Service,
    needs_socket_stubs: bool,
) -> String {
    let mut out = Vec::new();
    if needs_headers || needs_meta {
        out.push("use tonic::metadata::MetadataMap;".to_string());
    }
    if needs_rate_limit {
        out.push("use std::sync::{Mutex, OnceLock};".to_string());
        out.push("use std::time::{Duration, Instant};".to_string());
    }
    if service
        .rpc
        .grpc_config
        .as_ref()
        .and_then(|c| c.tls_cert.as_ref())
        .is_some()
    {
        out.push("use tonic::transport::ServerTlsConfig;".to_string());
        out.push("use tonic::transport::Identity;".to_string());
    }
    if needs_socket_stubs {
        out.push("use serde_json::Value;".to_string());
    }
    out.join("\n")
}

fn render_socket_stub_types(service: &Service) -> String {
    if service.sockets.sockets.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for socket in &service.sockets.sockets {
        let ctx_name = format!("{}SocketContext", socket.name.code_name());
        out.push_str(&format!(
            "#[derive(Clone)]\npub struct {ctx};\n\nimpl {ctx} {{\n    pub fn send_raw(&self, _kind: &str, _data: Value) {{}}\n    pub fn send_error(&self, _message: &str) {{}}\n",
            ctx = ctx_name
        ));
        for msg in &socket.outbound {
            out.push_str(&format!(
                "    pub fn send_{name}(&self, _data: Value) {{}}\n",
                name = msg.name.code_name()
            ));
        }
        out.push_str("}\n\n");
    }
    out
}

fn to_snake_case(input: &str) -> String {
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

fn render_rpc_header_struct(rpc: &RpcDef) -> String {
    if rpc.headers.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&format!("struct {}Headers {{\n", rpc.code_name()));
    for field in &rpc.headers {
        let ty = rust_type(&field.ty);
        if field.optional {
            out.push_str(&format!("    {}: Option<{}>,\n", field.name, ty));
        } else {
            out.push_str(&format!("    {}: {},\n", field.name, ty));
        }
    }
    out.push_str("}\n\n");
    out
}

fn render_headers_init(rpc: &RpcDef) -> String {
    if rpc.headers.is_empty() {
        return String::new();
    }
    let mut lines = Vec::new();
    for field in &rpc.headers {
        let expr = match field.ty {
            Type::String => format!("meta_str(metadata, \"{}\")", field.name),
            Type::Int => format!("meta_parse::<i64>(metadata, \"{}\")", field.name),
            Type::Float => format!("meta_parse::<f64>(metadata, \"{}\")", field.name),
            Type::Bool => format!("meta_parse::<bool>(metadata, \"{}\")", field.name),
            _ => format!("meta_str(metadata, \"{}\")", field.name),
        };
        let value = if field.optional {
            expr
        } else {
            format!(
                "{expr}.ok_or_else(|| Status::invalid_argument(\"missing header {name}\"))?",
                expr = expr,
                name = field.name
            )
        };
        lines.push(format!("            {}: {},", field.name, value));
    }
    format!(
        "        let headers = {}Headers {{\n{}\n        }};\n",
        rpc.code_name(),
        lines.join("\n")
    )
}

fn rust_type(ty: &Type) -> String {
    match ty {
        Type::String => "String".to_string(),
        Type::Int => "i64".to_string(),
        Type::Float => "f64".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Object(_) => "serde_json::Value".to_string(),
        Type::Array(inner) => format!("Vec<{}>", rust_type(inner)),
        Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) => {
            "serde_json::Value".to_string()
        }
        Type::Any => "serde_json::Value".to_string(),
        Type::Void => "()".to_string(),
        Type::Named(name) => name.code_name(),
    }
}

fn effective_auth<'a>(rpc: &'a RpcDef, service: &'a Service) -> Option<&'a AuthSpec> {
    match rpc.auth.as_ref() {
        Some(AuthSpec::None) => None,
        Some(auth) => Some(auth),
        None => match service.common.auth.as_ref() {
            Some(AuthSpec::None) => None,
            other => other,
        },
    }
}

fn render_auth_check(rpc: &RpcDef, service: &Service) -> String {
    let auth = match effective_auth(rpc, service) {
        Some(a) => a,
        None => return String::new(),
    };
    match auth {
        AuthSpec::Bearer => {
            "        let token = meta_str(metadata, \"authorization\");\n        if token.is_none() {\n            return Err(Status::unauthenticated(\"missing authorization\"));\n        }\n".to_string()
        }
        AuthSpec::ApiKey => {
            "        let token = meta_str(metadata, \"x-api-key\");\n        if token.is_none() {\n            return Err(Status::unauthenticated(\"missing api key\"));\n        }\n".to_string()
        }
        AuthSpec::None => String::new(),
    }
}

fn render_middleware_calls(rpc: &RpcDef, service: &Service) -> String {
    let mut out = String::new();
    for name in service
        .common
        .middleware
        .iter()
        .chain(rpc.middleware.iter())
    {
        out.push_str(&format!(
            "        {name}(&RpcContext {{ metadata }}).await?;\n",
            name = name
        ));
    }
    out
}

fn effective_rate_limit<'a>(rpc: &'a RpcDef, service: &'a Service) -> Option<&'a RateLimit> {
    rpc.rate_limit
        .as_ref()
        .or(service.common.rate_limit.as_ref())
}

fn render_rate_limit_helpers(service: &Service) -> String {
    let mut out = String::new();
    for rpc in &service.rpc.methods {
        if effective_rate_limit(rpc, service).is_some() {
            let static_name = format!("RATE_LIMIT_{}", rpc.code_name().to_uppercase());
            out.push_str(&format!(
                "static {static_name}: OnceLock<Mutex<(u32, Instant)>> = OnceLock::new();\n",
                static_name = static_name
            ));
        }
    }
    out.push('\n');
    out
}

fn render_rate_limit_check(rpc: &RpcDef, service: &Service) -> String {
    let Some(limit) = effective_rate_limit(rpc, service) else {
        return String::new();
    };
    let static_name = format!("RATE_LIMIT_{}", rpc.code_name().to_uppercase());
    format!(
        "        let state = {static_name}.get_or_init(|| Mutex::new((0, Instant::now())));\n        let mut guard = state.lock().unwrap();\n        let elapsed = guard.1.elapsed();\n        if elapsed >= Duration::from_secs({window}) {{\n            *guard = (0, Instant::now());\n        }}\n        if guard.0 >= {max} {{\n            return Err(Status::resource_exhausted(\"rate limited\"));\n        }}\n        guard.0 += 1;\n        drop(guard);\n",
        static_name = static_name,
        max = limit.max,
        window = limit.per_seconds
    )
}

fn grpc_addr_port(service: &Service) -> (String, u16) {
    if let Some(cfg) = &service.rpc.grpc_config {
        let addr = cfg
            .address
            .clone()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let port = cfg.port.unwrap_or(50051);
        return (addr, port);
    }
    ("127.0.0.1".to_string(), 50051)
}

fn render_tls_block(service: &Service) -> String {
    let Some(cfg) = &service.rpc.grpc_config else {
        return String::new();
    };
    let (Some(cert), Some(key)) = (cfg.tls_cert.as_ref(), cfg.tls_key.as_ref()) else {
        return String::new();
    };
    format!(
        "    let cert = std::fs::read(\"{cert}\")?;\n    let key = std::fs::read(\"{key}\")?;\n    let identity = Identity::from_pem(cert, key);\n    let tls = ServerTlsConfig::new().identity(identity);\n    builder = builder.tls_config(tls)?;\n",
        cert = cert,
        key = key
    )
}

fn render_max_message_block(service: &Service) -> String {
    let Some(cfg) = &service.rpc.grpc_config else {
        return String::new();
    };
    let Some(size) = cfg.max_message_size else {
        return String::new();
    };
    format!(
        "    service = service.max_decoding_message_size({size} as usize).max_encoding_message_size({size} as usize);\n",
        size = size
    )
}

fn needs_metadata(rpc: &RpcDef, service: &Service) -> bool {
    !rpc.headers.is_empty()
        || effective_auth(rpc, service).is_some()
        || !rpc.middleware.is_empty()
        || !service.common.middleware.is_empty()
}

fn build_middleware_rs(service: &Service) -> String {
    let mut names = std::collections::BTreeSet::new();
    for name in &service.common.middleware {
        names.insert(name.clone());
    }
    for rpc in &service.rpc.methods {
        for name in &rpc.middleware {
            names.insert(name.clone());
        }
    }
    if names.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("use tonic::Status;\nuse tonic::metadata::MetadataMap;\n\n");
    out.push_str("pub struct RpcContext<'a> { pub metadata: &'a MetadataMap }\n\n");
    for name in names {
        out.push_str(&format!(
            "pub async fn {name}(ctx: &RpcContext<'_>) -> Result<(), Status> {{\n    let key = format!(\"x-nimesvc-middleware-{{}}\", {name:?}.replace('_', \"-\"));\n    if let Some(raw) = ctx.metadata.get(&key).and_then(|v| v.to_str().ok()) {{\n        let value = raw.trim().to_ascii_lowercase();\n        if matches!(value.as_str(), \"block\" | \"deny\" | \"forbid\") {{\n            return Err(Status::permission_denied(format!(\"blocked by middleware {name}\")));\n        }}\n    }}\n    Ok(())\n}}\n\n",
            name = name
        ));
    }
    out
}
