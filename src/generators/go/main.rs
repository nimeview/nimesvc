use crate::ir::{
    CallArg, EventsBroker, InputRef, InputSource, ResponseSpec, Route, Service, Type, UseScope,
    effective_auth,
};

use super::sockets::{render_socket_helpers, socket_handler_name};
use super::util::{
    auth_middleware_name, body_struct_name, effective_rate_limit, go_call_name, go_field_name,
    go_type, handler_name, headers_struct_name, http_method, middleware_name, module_import_alias,
    needs_json, needs_rate_limit, needs_strconv, needs_types_in_main, path_struct_name,
    primary_response, query_struct_name, rate_limit_name, remote_call_fn_name,
    render_module_imports, to_go_func_name, wrap_optional,
};
use super::validation::render_validation_helpers;
use crate::generators::common::env::render_env_checks_go;
use crate::generators::common::errors::missing_header_status;
use crate::generators::common::headers::{
    default_cors_headers, default_cors_methods, header_runtime_key,
};
use crate::generators::common::validation::{route_has_regex, route_has_union_validation};

pub(super) fn build_main_go(service: &Service, module_name: &str) -> String {
    let mut imports = vec![
        "\"net/http\"".to_string(),
        "\"github.com/go-chi/chi/v5\"".to_string(),
    ];
    let needs_access_log = !service.http.routes.is_empty();
    let needs_ws = !service.sockets.sockets.is_empty();
    let needs_regex = service.http.routes.iter().any(route_has_regex);
    let needs_union = service.http.routes.iter().any(route_has_union_validation);
    if needs_json(service) || needs_ws {
        imports.push("\"encoding/json\"".to_string());
    }
    if needs_strconv(service) {
        imports.push("\"strconv\"".to_string());
    }
    if needs_rate_limit(service)
        || service
            .sockets
            .sockets
            .iter()
            .any(|s| s.rate_limit.is_some())
    {
        imports.push("\"sync\"".to_string());
        imports.push("\"time\"".to_string());
    }
    if needs_ws {
        imports.push("\"github.com/gorilla/websocket\"".to_string());
        if !imports.iter().any(|i| i == "\"fmt\"") {
            imports.push("\"fmt\"".to_string());
        }
        if !imports.iter().any(|i| i == "\"os\"") {
            imports.push("\"os\"".to_string());
        }
        if !imports.iter().any(|i| i == "\"sync\"") {
            imports.push("\"sync\"".to_string());
        }
    }
    if needs_regex {
        imports.push("\"regexp\"".to_string());
    }
    if needs_union {
        imports.push("\"math\"".to_string());
    }
    if !service.common.env.is_empty() {
        imports.push("\"fmt\"".to_string());
        imports.push("\"os\"".to_string());
    }
    if needs_access_log {
        if !imports.iter().any(|i| i == "\"fmt\"") {
            imports.push("\"fmt\"".to_string());
        }
        if !imports.iter().any(|i| i == "\"os\"") {
            imports.push("\"os\"".to_string());
        }
        if !imports.iter().any(|i| i == "\"time\"") {
            imports.push("\"time\"".to_string());
        }
    }
    if needs_types_in_main(service) {
        let types_import = format!("\"{}/types\"", module_name);
        imports.push(types_import);
    }
    imports.extend(render_module_imports(
        service,
        module_name,
        UseScope::Runtime,
    ));

    let mut route_structs = String::new();
    for route in &service.http.routes {
        route_structs.push_str(&render_route_input_structs(route));
    }

    let mut handlers = String::new();
    for route in &service.http.routes {
        handlers.push_str(&render_route_handler(route));
        handlers.push('\n');
    }
    let validation_helpers = render_validation_helpers(service);
    if !validation_helpers.is_empty() {
        handlers.push_str(&validation_helpers);
        handlers.push('\n');
    }
    if needs_ws {
        handlers.push_str(&render_socket_helpers(service));
        handlers.push('\n');
    }

    let rate_helpers = if needs_rate_limit(service) {
        render_rate_limit_helpers(service)
    } else {
        String::new()
    };
    let env_checks = render_env_checks_go(service);
    let cors_helpers = render_cors_helpers(service);
    let access_log_helpers = render_access_log_helpers(service);

    let mut router_setup = String::new();
    router_setup.push_str("r := chi.NewRouter()\n");
    if needs_rate_limit(service) {
        router_setup.push_str("    _ = rateLimitState\n");
    }
    if needs_access_log {
        router_setup.push_str("    r.Use(accessLogMiddleware)\n");
    }
    if service.common.cors.is_some() {
        router_setup.push_str("    r.Use(corsMiddleware)\n");
    }
    for name in &service.common.middleware {
        router_setup.push_str(&format!("r.Use({})\n", middleware_name(name)));
    }
    for route in &service.http.routes {
        let method = http_method(route);
        let path = &route.path;
        let handler = handler_name(route);
        let mut chain = Vec::new();
        if effective_rate_limit(route, service).is_some() {
            chain.push(rate_limit_name(route));
        }
        chain.extend(route.middleware.iter().map(|name| middleware_name(name)));
        if let Some(auth) = effective_auth(route.auth.as_ref(), service.common.auth.as_ref()) {
            chain.push(auth_middleware_name(auth));
        }
        if chain.is_empty() {
            router_setup.push_str(&format!(
                "r.Method(\"{method}\", \"{path}\", http.HandlerFunc({handler}))\n"
            ));
        } else {
            router_setup.push_str(&format!(
                "r.With({}).Method(\"{method}\", \"{path}\", http.HandlerFunc({handler}))\n",
                chain.join(", ")
            ));
        }
    }
    for socket in &service.sockets.sockets {
        router_setup.push_str(&format!(
            "r.Get(\"{}\", {})\n",
            socket.path,
            socket_handler_name(&socket.name.code_name())
        ));
    }

    let address = service.common.address.as_deref().unwrap_or("127.0.0.1");
    let port = service.common.port.unwrap_or(3000);

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

{rate_helpers}
{cors_helpers}
{access_log_helpers}
{route_structs}
{handlers}
func main() {{
{env_checks}
    {router_setup}
{event_consumers}
    http.ListenAndServe("{address}:{port}", r)
}}
"#,
        imports = imports
            .into_iter()
            .map(|i| format!("    {}", i))
            .collect::<Vec<_>>()
            .join("\n"),
        rate_helpers = rate_helpers,
        cors_helpers = cors_helpers,
        access_log_helpers = access_log_helpers,
        env_checks = env_checks,
        route_structs = route_structs,
        handlers = handlers,
        router_setup = router_setup,
        event_consumers = event_consumers,
        address = address,
        port = port
    )
}

fn render_access_log_helpers(service: &Service) -> String {
    if service.http.routes.is_empty() {
        return String::new();
    }
    format!(
        r#"type loggingResponseWriter struct {{
    http.ResponseWriter
    status int
}}

func (w *loggingResponseWriter) WriteHeader(status int) {{
    w.status = status
    w.ResponseWriter.WriteHeader(status)
}}

func accessLogMiddleware(next http.Handler) http.Handler {{
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {{
        started := time.Now()
        lw := &loggingResponseWriter{{ResponseWriter: w, status: http.StatusOK}}
        next.ServeHTTP(lw, r)
        fmt.Fprintf(os.Stderr, "[{service}] %s %s -> %d (%dms)\n", r.Method, r.URL.RequestURI(), lw.status, time.Since(started).Milliseconds())
    }})
}}

"#,
        service = service.name
    )
}

fn render_route_input_structs(route: &Route) -> String {
    let mut out = String::new();
    if !route.input.path.is_empty() {
        out.push_str(&format!("type {} struct {{\n", path_struct_name(route)));
        for field in &route.input.path {
            let go_name = go_field_name(&field.name);
            let go_ty = go_type(&field.ty, true);
            out.push_str(&format!("    {} {}\n", go_name, go_ty));
        }
        out.push_str("}\n\n");
    }
    if !route.input.query.is_empty() {
        out.push_str(&format!("type {} struct {{\n", query_struct_name(route)));
        for field in &route.input.query {
            let go_name = go_field_name(&field.name);
            let go_ty = go_type(&field.ty, true);
            if field.optional {
                out.push_str(&format!("    {} {}\n", go_name, wrap_optional(&go_ty)));
            } else {
                out.push_str(&format!("    {} {}\n", go_name, go_ty));
            }
        }
        out.push_str("}\n\n");
    }
    if !route.headers.is_empty() {
        out.push_str(&format!("type {} struct {{\n", headers_struct_name(route)));
        for field in &route.headers {
            let go_name = go_field_name(&field.name);
            let go_ty = go_type(&field.ty, true);
            if field.optional {
                out.push_str(&format!("    {} {}\n", go_name, wrap_optional(&go_ty)));
            } else {
                out.push_str(&format!("    {} {}\n", go_name, go_ty));
            }
        }
        out.push_str("}\n\n");
    }
    if !route.input.body.is_empty() {
        out.push_str(&format!("type {} struct {{\n", body_struct_name(route)));
        for field in &route.input.body {
            let go_name = go_field_name(&field.name);
            let go_ty = go_type(&field.ty, true);
            if field.optional {
                out.push_str(&format!(
                    "    {} {} `json:\"{},omitempty\"`\n",
                    go_name,
                    wrap_optional(&go_ty),
                    field.name
                ));
            } else {
                out.push_str(&format!(
                    "    {} {} `json:\"{}\"`\n",
                    go_name, go_ty, field.name
                ));
            }
        }
        out.push_str("}\n\n");
    }
    out
}

fn render_cors_helpers(service: &Service) -> String {
    let Some(cors) = &service.common.cors else {
        return String::new();
    };
    let allow_any = if cors.allow_any { "true" } else { "false" };
    let mut out = String::new();
    out.push_str(&format!("var corsAllowAny = {}\n", allow_any));
    if cors.allow_any {
        out.push_str("var corsAllowList = map[string]bool{}\n\n");
    } else {
        let items = cors
            .origins
            .iter()
            .map(|o| {
                format!(
                    "\"{}\": true",
                    o.replace('\\', "\\\\").replace('\"', "\\\"")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "var corsAllowList = map[string]bool{{{}}}\n\n",
            items
        ));
    }
    let methods = if cors.methods.is_empty() {
        format!("\"{}\"", default_cors_methods())
    } else {
        format!("\"{}\"", cors.methods.join(","))
    };
    let headers = if cors.headers.is_empty() {
        format!("\"{}\"", default_cors_headers())
    } else {
        format!("\"{}\"", cors.headers.join(","))
    };
    out.push_str(&format!(
        r#"func corsMiddleware(next http.Handler) http.Handler {{
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {{
        origin := r.Header.Get("Origin")
        if corsAllowAny {{
            w.Header().Set("Access-Control-Allow-Origin", "*")
        }} else if origin != "" && corsAllowList[origin] {{
            w.Header().Set("Access-Control-Allow-Origin", origin)
            w.Header().Set("Vary", "Origin")
        }}
        w.Header().Set("Access-Control-Allow-Headers", {headers})
        w.Header().Set("Access-Control-Allow-Methods", {methods})
        if r.Method == http.MethodOptions {{
            w.WriteHeader(http.StatusNoContent)
            return
        }}
        next.ServeHTTP(w, r)
    }})
}}

"#,
        headers = headers,
        methods = methods
    ));
    out
}

fn render_route_handler(route: &Route) -> String {
    let name = handler_name(route);
    let resp = primary_response(route);
    let mut out = String::new();
    out.push_str(&format!(
        "func {name}(w http.ResponseWriter, r *http.Request) {{\n",
        name = name
    ));

    if !route.input.path.is_empty() {
        out.push_str(&format!("    var path {};\n", path_struct_name(route)));
        for field in &route.input.path {
            let go_name = go_field_name(&field.name);
            let getter = format!("chi.URLParam(r, \"{}\")", field.name);
            out.push_str(&parse_from_string(
                &format!("path.{}", go_name),
                &field.ty,
                &getter,
                true,
                false,
                "http.StatusBadRequest",
                "\"missing parameter\"",
            ));
        }
    }
    if !route.input.query.is_empty() {
        out.push_str(&format!("    var query {};\n", query_struct_name(route)));
        out.push_str("    q := r.URL.Query()\n");
        for field in &route.input.query {
            let go_name = go_field_name(&field.name);
            let getter = format!("q.Get(\"{}\")", field.name);
            out.push_str(&parse_from_string(
                &format!("query.{}", go_name),
                &field.ty,
                &getter,
                !field.optional,
                field.optional,
                "http.StatusBadRequest",
                "\"missing parameter\"",
            ));
        }
    }
    if !route.headers.is_empty() {
        out.push_str(&format!(
            "    var headers {};\n",
            headers_struct_name(route)
        ));
        for field in &route.headers {
            let go_name = go_field_name(&field.name);
            let key = header_runtime_key(&field.name);
            let getter = format!("r.Header.Get(\"{}\")", key);
            let missing_status = if missing_header_status(&field.name) == 401 {
                "http.StatusUnauthorized"
            } else {
                "http.StatusBadRequest"
            };
            let missing_msg = format!("\"missing header {}\"", field.name);
            out.push_str(&parse_from_string(
                &format!("headers.{}", go_name),
                &field.ty,
                &getter,
                !field.optional,
                field.optional,
                missing_status,
                &missing_msg,
            ));
        }
    }
    if !route.input.body.is_empty() {
        out.push_str(&format!("    var body {};\n", body_struct_name(route)));
        out.push_str("    if err := json.NewDecoder(r.Body).Decode(&body); err != nil {\n");
        out.push_str("        http.Error(w, \"invalid body\", http.StatusBadRequest)\n");
        out.push_str("        return\n    }\n");
    }

    let validation_block = super::validation::render_validation_block(route);
    out.push_str(&validation_block);

    if route.healthcheck {
        out.push_str(&healthcheck_go(resp));
        out.push_str("}\n");
        return out;
    }

    if route.call.service_base.is_some() {
        let call_expr = render_call_expr(route);
        if resp.ty == Type::Void {
            out.push_str(&format!(
                "    if err := {call}; err != nil {{\n        http.Error(w, err.Error(), http.StatusBadGateway)\n        return\n    }}\n",
                call = call_expr.trim()
            ));
            out.push_str(&format!("    w.WriteHeader({})\n", resp.status));
        } else {
            out.push_str(&format!(
                "    result, err := {call}\n    if err != nil {{\n        http.Error(w, err.Error(), http.StatusBadGateway)\n        return\n    }}\n",
                call = call_expr.trim()
            ));
            out.push_str(&render_write_response(resp));
        }
    } else {
        let call_expr = render_call_expr(route);
        if resp.ty == Type::Void {
            out.push_str(&call_expr);
            out.push_str(&format!("    w.WriteHeader({})\n", resp.status));
        } else {
            out.push_str(&format!("    result := {call}\n", call = call_expr.trim()));
            out.push_str(&render_write_response(resp));
        }
    }

    out.push_str("}\n");
    out
}

fn render_call_expr(route: &Route) -> String {
    if route.call.service_base.is_some() {
        let name = remote_call_fn_name(route);
        let args = render_call_args(&route.call.args);
        return format!("{name}({args}, r)\n", name = name, args = args);
    }
    let module = &route.call.module;
    let func = if module.is_empty() {
        to_go_func_name(&route.call.function)
    } else {
        go_call_name(&route.call.function)
    };
    let args = render_call_args(&route.call.args);
    if module.is_empty() {
        format!("{}({})\n", func, args)
    } else {
        let pkg = module_import_alias(module);
        format!(
            "{pkg}.{func}({args})\n",
            pkg = pkg,
            func = func,
            args = args
        )
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
    let base = match input.source {
        InputSource::Path => "path",
        InputSource::Query => "query",
        InputSource::Body => "body",
        InputSource::Headers => "headers",
        InputSource::Input => "input",
    };
    if input.path.is_empty() {
        base.to_string()
    } else {
        let mut parts = Vec::new();
        for p in &input.path {
            parts.push(go_field_name(p));
        }
        format!("{}.{}", base, parts.join("."))
    }
}

fn render_write_response(resp: &ResponseSpec) -> String {
    let mut out = String::new();
    match resp.ty {
        Type::String => {
            out.push_str("    w.Header().Set(\"Content-Type\", \"text/plain\")\n");
            out.push_str(&format!("    w.WriteHeader({})\n", resp.status));
            out.push_str("    _, _ = w.Write([]byte(result))\n");
        }
        _ => {
            out.push_str("    w.Header().Set(\"Content-Type\", \"application/json\")\n");
            out.push_str(&format!("    w.WriteHeader({})\n", resp.status));
            out.push_str("    _ = json.NewEncoder(w).Encode(result)\n");
        }
    }
    out
}

fn healthcheck_go(resp: &ResponseSpec) -> String {
    if resp.ty == Type::Void {
        format!("    w.WriteHeader({})\n", resp.status)
    } else {
        format!(
            "    w.WriteHeader({})\n    _, _ = w.Write([]byte(\"ok\"))\n",
            resp.status
        )
    }
}

fn parse_from_string(
    target: &str,
    ty: &Type,
    getter: &str,
    required: bool,
    optional_ptr: bool,
    missing_status: &str,
    missing_msg: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("    value := {}\n", getter));
    if required {
        out.push_str("    if value == \"\" {\n");
        out.push_str(&format!(
            "        http.Error(w, {}, {})\n",
            missing_msg, missing_status
        ));
        out.push_str("        return\n    }\n");
    } else {
        out.push_str("    if value != \"\" {\n");
    }
    match ty {
        Type::String => {
            if optional_ptr {
                out.push_str(&format!("    {} = &value\n", target));
            } else {
                out.push_str(&format!("    {} = value\n", target));
            }
        }
        Type::Int => {
            out.push_str("    parsed, err := strconv.ParseInt(value, 10, 64)\n");
            out.push_str("    if err != nil { http.Error(w, \"invalid int\", http.StatusBadRequest); return }\n");
            if optional_ptr {
                out.push_str("    tmp := parsed\n");
                out.push_str(&format!("    {} = &tmp\n", target));
            } else {
                out.push_str(&format!("    {} = parsed\n", target));
            }
        }
        Type::Float => {
            out.push_str("    parsed, err := strconv.ParseFloat(value, 64)\n");
            out.push_str("    if err != nil { http.Error(w, \"invalid float\", http.StatusBadRequest); return }\n");
            if optional_ptr {
                out.push_str("    tmp := parsed\n");
                out.push_str(&format!("    {} = &tmp\n", target));
            } else {
                out.push_str(&format!("    {} = parsed\n", target));
            }
        }
        Type::Bool => {
            out.push_str("    parsed, err := strconv.ParseBool(value)\n");
            out.push_str("    if err != nil { http.Error(w, \"invalid bool\", http.StatusBadRequest); return }\n");
            if optional_ptr {
                out.push_str("    tmp := parsed\n");
                out.push_str(&format!("    {} = &tmp\n", target));
            } else {
                out.push_str(&format!("    {} = parsed\n", target));
            }
        }
        _ => {
            out.push_str(&format!(
                "    var parsed {}\n    if err := json.Unmarshal([]byte(value), &parsed); err != nil {{ http.Error(w, \"invalid json value\", http.StatusBadRequest); return }}\n",
                go_type(ty, needs_types_pkg_for_parse(ty))
            ));
            if optional_ptr {
                out.push_str("    tmp := parsed\n");
                out.push_str(&format!("    {} = &tmp\n", target));
            } else {
                out.push_str(&format!("    {} = parsed\n", target));
            }
        }
    }
    if !required {
        out.push_str("    }\n");
    }
    out
}

fn needs_types_pkg_for_parse(ty: &Type) -> bool {
    match ty {
        Type::Named(_) => true,
        Type::Array(inner) | Type::Map(inner) | Type::Nullable(inner) => {
            needs_types_pkg_for_parse(inner)
        }
        Type::Union(types) | Type::OneOf(types) => types.iter().any(needs_types_pkg_for_parse),
        _ => false,
    }
}

fn render_rate_limit_helpers(service: &Service) -> String {
    let mut out = String::new();
    out.push_str("type rateLimitStateType struct { count int; reset time.Time }\n");
    out.push_str("var rateLimitState = struct { sync.Mutex; m map[string]*rateLimitStateType }{m: map[string]*rateLimitStateType{}}\n\n");
    for route in &service.http.routes {
        if let Some(limit) = effective_rate_limit(route, service) {
            let name = rate_limit_name(route);
            out.push_str(&format!(
                "func {name}(next http.Handler) http.Handler {{\n    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {{\n        rateLimitState.Lock()\n        state, ok := rateLimitState.m[\"{key}\"]\n        if !ok || time.Since(state.reset) >= {window}*time.Second {{\n            state = &rateLimitStateType{{count: 0, reset: time.Now()}}\n            rateLimitState.m[\"{key}\"] = state\n        }}\n        if state.count >= {max} {{\n            rateLimitState.Unlock()\n            http.Error(w, \"Too Many Requests\", http.StatusTooManyRequests)\n            return\n        }}\n        state.count++\n        rateLimitState.Unlock()\n        next.ServeHTTP(w, r)\n    }})\n}}\n\n",
                name = name,
                key = name,
                window = limit.per_seconds,
                max = limit.max
            ));
        }
    }
    out
}

// env checks are rendered via generators/common/env.rs
