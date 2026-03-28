use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::generators::go::events::build_events_go;
use crate::generators::go::util::{go_field_name, go_type, type_uses_types_pkg};
use crate::generators::rpc;
use crate::ir::{
    AuthSpec, CallArg, EventsBroker, InputRef, InputSource, RateLimit, RpcDef, Service, Type,
};

pub(super) fn generate(service: &Service, out_dir: &Path) -> Result<()> {
    let proto_dir = out_dir.join("proto");
    fs::create_dir_all(&proto_dir)
        .with_context(|| format!("Failed to create '{}'", proto_dir.display()))?;

    let module_name = format!("nimesvc/{}-grpc", service.name.to_lowercase());
    let needs_redis = matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    );
    let mut extra_deps = String::new();
    if needs_redis {
        extra_deps.push_str("    github.com/redis/go-redis/v9 v9.5.1\n");
    }
    let go_mod = format!(
        "module {}\n\ngo 1.20\n\nrequire (\n    google.golang.org/grpc v1.64.0\n    google.golang.org/protobuf v1.34.0\n{} )\n",
        module_name, extra_deps
    );
    fs::write(out_dir.join("go.mod"), go_mod).with_context(|| "Failed to write go.mod")?;

    let proto = rpc::build_proto(service);
    fs::write(proto_dir.join("rpc.proto"), proto)
        .with_context(|| "Failed to write proto/rpc.proto")?;

    let types_go = build_types_go(service, &module_name);
    let types_dir = out_dir.join("types");
    fs::create_dir_all(&types_dir)
        .with_context(|| format!("Failed to create '{}'", types_dir.display()))?;
    fs::write(types_dir.join("types.go"), types_go)
        .with_context(|| "Failed to write types/types.go")?;
    if !service.events.definitions.is_empty() {
        let events_go = build_events_go(service, &module_name);
        fs::write(out_dir.join("events.go"), events_go)
            .with_context(|| "Failed to write events.go")?;
    }

    let gen_sh = r#"#!/usr/bin/env bash
set -e
protoc \
  --go_out=. --go_opt=paths=source_relative \
  --go-grpc_out=. --go-grpc_opt=paths=source_relative \
  proto/rpc.proto
"#;
    fs::write(out_dir.join("gen.sh"), gen_sh).with_context(|| "Failed to write gen.sh")?;

    let server_go = build_server_go(service, &module_name);
    fs::write(out_dir.join("server.go"), server_go).with_context(|| "Failed to write server.go")?;

    let main_go = build_main_go(service, &module_name);
    fs::write(out_dir.join("main.go"), main_go).with_context(|| "Failed to write main.go")?;

    let middleware_go = build_middleware_go(service);
    if !middleware_go.is_empty() {
        fs::write(out_dir.join("middleware.go"), middleware_go)
            .with_context(|| "Failed to write middleware.go")?;
    }

    Ok(())
}

fn build_types_go(service: &Service, module_name: &str) -> String {
    let mut out = String::new();
    out.push_str("package types\n\n");
    out.push_str(&format!("import rpc \"{}/proto\"\n\n", module_name));
    for en in &service.schema.enums {
        out.push_str(&format!(
            "type {} = rpc.{}\n",
            en.name.code_name(),
            en.name.code_name()
        ));
    }
    for ty in &service.schema.types {
        out.push_str(&format!(
            "type {} = rpc.{}\n",
            ty.name.code_name(),
            ty.name.code_name()
        ));
    }
    for rpc in &service.rpc.methods {
        out.push_str(&format!(
            "type {}Request = rpc.{}Request\n",
            rpc.code_name(),
            rpc.code_name()
        ));
        out.push_str(&format!(
            "type {}Response = rpc.{}Response\n",
            rpc.code_name(),
            rpc.code_name()
        ));
    }
    out
}

fn build_main_go(service: &Service, module_name: &str) -> String {
    let (addr, port) = grpc_addr_port(service);
    let (tls_enabled, cert, key) = grpc_tls(service);
    let max_msg = grpc_max_message(service);
    let mut imports = vec![
        "\"fmt\"".to_string(),
        "\"net\"".to_string(),
        "\"google.golang.org/grpc\"".to_string(),
        format!("rpc \"{}/proto\"", module_name),
    ];
    if tls_enabled {
        imports.push("\"google.golang.org/grpc/credentials\"".to_string());
    }
    let needs_event_consumers = matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    ) && !service.events.subscribes.is_empty();
    let event_consumers = if needs_event_consumers {
        "    StartEventConsumers()\n"
    } else {
        ""
    };
    format!(
        r#"package main

import (
{imports}
)

func main() {{
    lis, err := net.Listen("tcp", "{addr}:{port}")
    if err != nil {{
        panic(err)
    }}

    opts := []grpc.ServerOption{{}}
{max_msg}{tls_opts}
    s := grpc.NewServer(opts...)
    rpc.Register{service_name}Server(s, &Server{{}})
{event_consumers}
    fmt.Println("gRPC server listening on {addr}:{port}")
    if err := s.Serve(lis); err != nil {{
        panic(err)
    }}
}}
"#,
        imports = imports
            .into_iter()
            .map(|i| format!("    {}", i))
            .collect::<Vec<_>>()
            .join("\n"),
        addr = addr,
        port = port,
        max_msg = max_msg,
        tls_opts = render_tls_opts(tls_enabled, cert, key),
        service_name = service.name,
        event_consumers = event_consumers,
    )
}

fn build_server_go(service: &Service, module_name: &str) -> String {
    let needs_access_log = !service.rpc.methods.is_empty();
    let needs_headers = service.rpc.methods.iter().any(|r| !r.headers.is_empty());
    let needs_auth =
        service.common.auth.is_some() || service.rpc.methods.iter().any(|r| r.auth.is_some());
    let needs_rate_limit = service
        .rpc
        .methods
        .iter()
        .any(|r| effective_rate_limit(r, service).is_some());
    let needs_metadata = needs_headers || needs_auth;
    let needs_status = needs_headers || needs_auth || needs_rate_limit;
    let needs_types = service
        .rpc
        .methods
        .iter()
        .any(|r| r.headers.iter().any(|f| type_uses_types_pkg(&f.ty)));
    let needs_json = service.rpc.methods.iter().any(|r| {
        matches!(
            r.output,
            Type::Named(_)
                | Type::Object(_)
                | Type::Any
                | Type::Map(_)
                | Type::Union(_)
                | Type::OneOf(_)
                | Type::Nullable(_)
        )
    });
    let needs_structpb = service.rpc.methods.iter().any(|r| {
        matches!(
            r.output,
            Type::Any | Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_)
        )
    });

    let mut imports = vec!["\"context\"".to_string()];
    if needs_access_log {
        imports.push("\"fmt\"".to_string());
        imports.push("\"os\"".to_string());
        imports.push("\"time\"".to_string());
    }
    if needs_json {
        imports.push("\"encoding/json\"".to_string());
    }
    if needs_rate_limit {
        imports.push("\"time\"".to_string());
    }
    if needs_headers {
        imports.push("\"strconv\"".to_string());
    }
    imports.push(format!("rpc \"{}/proto\"", module_name));
    if needs_types {
        imports.push(format!("types \"{}/types\"", module_name));
    }
    if needs_metadata {
        imports.push("\"google.golang.org/grpc/metadata\"".to_string());
    }
    if needs_status {
        imports.push("\"google.golang.org/grpc/status\"".to_string());
        imports.push("\"google.golang.org/grpc/codes\"".to_string());
    } else if needs_access_log {
        imports.push("\"google.golang.org/grpc/status\"".to_string());
    }
    if needs_structpb {
        imports.push("\"google.golang.org/protobuf/types/known/structpb\"".to_string());
    }
    let mut seen_modules = std::collections::HashSet::new();
    for module in &service.common.modules {
        let local = module.alias.as_deref().unwrap_or(&module.name);
        let key = format!("{}|{}|{}", local, module.name, module.path.is_some());
        if !seen_modules.insert(key) {
            continue;
        }
        if module.path.is_some() {
            let import_path = format!("{}/modules/{}", module_name, local);
            imports.push(format!("{} \"{}\"", local, import_path));
        } else {
            imports.push(format!("{} \"{}\"", local, module.name));
        }
    }

    let mut methods = String::new();
    for rpc in &service.rpc.methods {
        methods.push_str(&render_rpc_method(rpc, service));
        methods.push('\n');
    }

    let mut helpers = String::new();
    if needs_headers {
        helpers.push_str(&render_header_structs(service));
        helpers.push('\n');
        helpers.push_str(&render_header_helpers());
    }
    if !needs_headers
        && (service.common.auth.is_some() || service.rpc.methods.iter().any(|r| r.auth.is_some()))
    {
        helpers.push_str("func metaGet(md metadata.MD, name string) (string, bool) {\n    vals := md.Get(name)\n    if len(vals) == 0 {\n        return \"\", false\n    }\n    return vals[0], true\n}\n\n");
    }
    if needs_rate_limit {
        helpers.push_str(&render_rate_limit_helpers(service));
    }
    helpers.push_str(&render_auth_helpers(service));

    format!(
        r#"package main

import (
{imports}
)

type Server struct {{
    rpc.Unimplemented{service_name}Server
}}

{methods}
{helpers}
"#,
        imports = imports
            .into_iter()
            .map(|i| format!("    {}", i))
            .collect::<Vec<_>>()
            .join("\n"),
        service_name = service.name,
        methods = methods,
        helpers = helpers
    )
}

fn render_rpc_method(rpc: &RpcDef, service: &Service) -> String {
    let method = rpc.code_name();
    let req = format!("*rpc.{}Request", rpc.code_name());
    let resp_ty = rpc_response_type(rpc);
    let call_expr = render_call_expr(rpc);
    let (response_preamble, response_expr) = render_response_expr(rpc);
    let auth = render_auth_check(rpc, service);
    let middleware_calls = render_middleware_calls(rpc, service);
    let rate_limit = render_rate_limit_check(rpc, service);
    let headers_init = render_headers_init(rpc);

    format!(
        "func (s *Server) {method}(ctx context.Context, input {req}) (resp {resp_ty}, err error) {{\n    started := time.Now()\n    defer func() {{\n        statusLabel := \"OK\"\n        if err != nil {{\n            statusLabel = status.Code(err).String()\n        }}\n        fmt.Fprintf(os.Stderr, \"[{service_name}] gRPC {rpc_name} -> %s (%dms)\\n\", statusLabel, time.Since(started).Milliseconds())\n    }}()\n{auth}{headers_init}{middleware_calls}{rate_limit}    result := {call_expr}\n{response_preamble}    return {response_expr}, nil\n}}",
        method = method,
        req = req,
        resp_ty = resp_ty,
        call_expr = call_expr,
        response_expr = response_expr,
        response_preamble = response_preamble,
        auth = auth,
        headers_init = headers_init,
        middleware_calls = middleware_calls,
        rate_limit = rate_limit,
        service_name = service.name,
        rpc_name = rpc.code_name(),
    )
}

fn rpc_response_type(rpc: &RpcDef) -> String {
    match &rpc.output {
        Type::Void => "*rpc.Empty".to_string(),
        Type::Object(_) => format!("*rpc.{}Response", rpc.code_name()),
        Type::Any | Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) => {
            "*rpc.Struct".to_string()
        }
        Type::Named(name) => format!("*rpc.{}", name.code_name()),
        _ => format!("*rpc.{}Response", rpc.code_name()),
    }
}

fn render_response_expr(rpc: &RpcDef) -> (String, String) {
    match &rpc.output {
        Type::Void => (String::new(), "&rpc.Empty{}".to_string()),
        Type::Named(name) => {
            let ty = name.code_name();
            (
                format!(
                    "    var response rpc.{ty}\n    if data, err := json.Marshal(result); err == nil {{\n        _ = json.Unmarshal(data, &response)\n    }}\n",
                    ty = ty
                ),
                "&response".to_string(),
            )
        }
        Type::Object(_) => {
            let ty = format!("{}Response", rpc.code_name());
            (
                format!(
                    "    var response rpc.{ty}\n    if data, err := json.Marshal(result); err == nil {{\n        _ = json.Unmarshal(data, &response)\n    }}\n",
                    ty = ty
                ),
                "&response".to_string(),
            )
        }
        Type::Any | Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) => (
            "    data, _ := json.Marshal(result)\n    var tmp map[string]any\n    _ = json.Unmarshal(data, &tmp)\n    response, _ := structpb.NewStruct(tmp)\n".to_string(),
            "response".to_string(),
        ),
        Type::Array(_) | Type::String | Type::Int | Type::Float | Type::Bool => (
            String::new(),
            format!("&rpc.{}Response{{Value: result}}", rpc.code_name()),
        ),
    }
}

fn render_call_expr(rpc: &RpcDef) -> String {
    let module = &rpc.call.module;
    let func = &rpc.call.function;
    let args = render_call_args(&rpc.call.args);
    format!(
        "{module}.{func}({args})",
        module = module,
        func = func,
        args = args
    )
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
        let path = input
            .path
            .iter()
            .map(|seg| go_field_name(seg))
            .collect::<Vec<_>>()
            .join(".");
        return format!("{}.{}", base, path);
    }
    if input.path.is_empty() {
        return "input".to_string();
    }
    let path = input
        .path
        .iter()
        .map(|seg| go_field_name(seg))
        .collect::<Vec<_>>()
        .join(".");
    format!("input.{}", path)
}

fn render_header_structs(service: &Service) -> String {
    let mut out = String::new();
    for rpc in &service.rpc.methods {
        if rpc.headers.is_empty() {
            continue;
        }
        out.push_str(&format!("type {}Headers struct {{\n", rpc.code_name()));
        for field in &rpc.headers {
            let name = go_field_name(&field.name);
            let ty = go_type(&field.ty, true);
            if field.optional {
                out.push_str(&format!("    {} *{}\n", name, ty));
            } else {
                out.push_str(&format!("    {} {}\n", name, ty));
            }
        }
        out.push_str("}\n\n");
    }
    out
}

fn render_header_helpers() -> String {
    r#"
func metaGet(md metadata.MD, name string) (string, bool) {
    vals := md.Get(name)
    if len(vals) == 0 {
        return "", false
    }
    return vals[0], true
}

func parseBool(v string) (bool, bool) {
    if v == "true" {
        return true, true
    }
    if v == "false" {
        return false, true
    }
    return false, false
}
"#
    .to_string()
}

fn render_headers_init(rpc: &RpcDef) -> String {
    if rpc.headers.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("    md, _ := metadata.FromIncomingContext(ctx)\n");
    out.push_str(&format!("    var headers {}Headers\n", rpc.code_name()));
    for field in &rpc.headers {
        let name = go_field_name(&field.name);
        let getter = format!("metaGet(md, \"{}\")", field.name);
        let required = !field.optional;
        out.push_str(&render_parse_from_string(
            &format!("headers.{}", name),
            &field.ty,
            &getter,
            required,
            field.optional,
        ));
    }
    out
}

fn render_parse_from_string(
    target: &str,
    ty: &Type,
    getter: &str,
    required: bool,
    optional: bool,
) -> String {
    match ty {
        Type::String => render_parse_string(target, getter, required, optional, None),
        Type::Int => render_parse_string(target, getter, required, optional, Some("int64")),
        Type::Float => render_parse_string(target, getter, required, optional, Some("float64")),
        Type::Bool => render_parse_string(target, getter, required, optional, Some("bool")),
        _ => render_parse_string(target, getter, required, optional, None),
    }
}

fn render_parse_string(
    target: &str,
    getter: &str,
    required: bool,
    optional: bool,
    kind: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("    if val, ok := {getter}; ok {{\n"));
    match kind {
        Some("int64") => out.push_str(&format!(
            "        if parsed, err := strconv.ParseInt(val, 10, 64); err == nil {{\n"
        )),
        Some("float64") => out.push_str(&format!(
            "        if parsed, err := strconv.ParseFloat(val, 64); err == nil {{\n"
        )),
        Some("bool") => out.push_str("        if parsed, ok := parseBool(val); ok {\n"),
        _ => out.push_str("        {\n            parsed := val\n"),
    }
    if optional {
        out.push_str(&format!("            {target} = &parsed\n"));
    } else {
        out.push_str(&format!("            {target} = parsed\n"));
    }
    out.push_str("        }\n");
    out.push_str("    } else {\n");
    if required {
        out.push_str(
            "        return nil, status.Error(codes.InvalidArgument, \"missing header\")\n",
        );
    }
    out.push_str("    }\n");
    out
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
            "    if err := {name}(ctx); err != nil {{\n        return nil, err\n    }}\n",
            name = name
        ));
    }
    out
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
    let Some(auth) = effective_auth(rpc, service) else {
        return String::new();
    };
    match auth {
        AuthSpec::Bearer => {
            "    if err := authBearer(ctx); err != nil {\n        return nil, err\n    }\n"
                .to_string()
        }
        AuthSpec::ApiKey => {
            "    if err := authApiKey(ctx); err != nil {\n        return nil, err\n    }\n"
                .to_string()
        }
        AuthSpec::None => String::new(),
    }
}

fn render_auth_helpers(service: &Service) -> String {
    if service.common.auth.is_none() && service.rpc.methods.iter().all(|r| r.auth.is_none()) {
        return String::new();
    }
    r#"
func authBearer(ctx context.Context) error {
    md, _ := metadata.FromIncomingContext(ctx)
    if _, ok := metaGet(md, "authorization"); !ok {
        return status.Error(codes.Unauthenticated, "missing authorization")
    }
    return nil
}

func authApiKey(ctx context.Context) error {
    md, _ := metadata.FromIncomingContext(ctx)
    if _, ok := metaGet(md, "x-api-key"); !ok {
        return status.Error(codes.Unauthenticated, "missing api key")
    }
    return nil
}
"#
    .to_string()
}

fn effective_rate_limit<'a>(rpc: &'a RpcDef, service: &'a Service) -> Option<&'a RateLimit> {
    rpc.rate_limit
        .as_ref()
        .or(service.common.rate_limit.as_ref())
}

fn render_rate_limit_helpers(service: &Service) -> String {
    let mut out = String::new();
    out.push_str(
        "type rateLimit struct { count int; reset time.Time; max int; window time.Duration }\n",
    );
    out.push_str("func (r *rateLimit) Allow() bool {\n    now := time.Now()\n    if r.reset.IsZero() || now.After(r.reset) {\n        r.reset = now.Add(r.window)\n        r.count = 0\n    }\n    if r.count >= r.max { return false }\n    r.count += 1\n    return true\n}\n\n");
    for rpc in &service.rpc.methods {
        if let Some(limit) = effective_rate_limit(rpc, service) {
            out.push_str(&format!(
                "var rateLimit{code} = rateLimit{{max: {max}, window: time.Duration({window}) * time.Second}}\n",
                code = rpc.code_name(),
                max = limit.max,
                window = limit.per_seconds
            ));
        }
    }
    out.push('\n');
    out
}

fn render_rate_limit_check(rpc: &RpcDef, service: &Service) -> String {
    if effective_rate_limit(rpc, service).is_none() {
        return String::new();
    }
    format!(
        "    if !rateLimit{code}.Allow() {{\n        return nil, status.Error(codes.ResourceExhausted, \"rate limited\")\n    }}\n",
        code = rpc.code_name()
    )
}

fn build_middleware_go(service: &Service) -> String {
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
    out.push_str("package main\n\nimport (\n    \"context\"\n    \"strings\"\n    \"google.golang.org/grpc/codes\"\n    \"google.golang.org/grpc/metadata\"\n    \"google.golang.org/grpc/status\"\n)\n\n");
    for name in names {
        out.push_str(&format!(
            "func {name}(ctx context.Context) error {{\n    md, _ := metadata.FromIncomingContext(ctx)\n    key := \"x-nimesvc-middleware-\" + strings.ReplaceAll({name:?}, \"_\", \"-\")\n    if value, ok := metaGet(md, key); ok {{\n        value = strings.ToLower(strings.TrimSpace(value))\n        if value == \"block\" || value == \"deny\" || value == \"forbid\" {{\n            return status.Error(codes.PermissionDenied, \"blocked by middleware {name}\")\n        }}\n    }}\n    return nil\n}}\n\n",
            name = name
        ));
    }
    out
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

fn grpc_tls(service: &Service) -> (bool, String, String) {
    if let Some(cfg) = &service.rpc.grpc_config {
        if let (Some(cert), Some(key)) = (cfg.tls_cert.clone(), cfg.tls_key.clone()) {
            return (true, cert, key);
        }
    }
    (false, String::new(), String::new())
}

fn grpc_max_message(service: &Service) -> String {
    if let Some(cfg) = &service.rpc.grpc_config {
        if let Some(size) = cfg.max_message_size {
            return format!(
                "    opts = append(opts, grpc.MaxRecvMsgSize(int({size})), grpc.MaxSendMsgSize(int({size})))\n",
                size = size
            );
        }
    }
    String::new()
}

fn render_tls_opts(enabled: bool, cert: String, key: String) -> String {
    if !enabled {
        return String::new();
    }
    format!(
        "    creds, err := credentials.NewServerTLSFromFile(\"{cert}\", \"{key}\")\n    if err != nil {{\n        panic(err)\n    }}\n    opts = append(opts, grpc.Creds(creds))\n",
        cert = cert,
        key = key
    )
}
