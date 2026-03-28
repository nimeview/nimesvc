use crate::generators::common::errors::missing_header_status;
use crate::generators::common::headers::header_runtime_key;
use crate::ir::{
    CallArg, CallSpec, Field, InputRef, InputSource, RateLimit, ResponseSpec, Route, Service, Type,
    effective_auth,
};

use super::remote_calls::remote_call_fn_name;
use super::util::{handler_name, rust_type, status_code_expr, to_snake_case};
use super::validation;

pub(super) fn render_route_helpers(service: &Service) -> String {
    let mut out = String::new();
    for route in &service.http.routes {
        out.push_str(&render_input_structs(route));
    }
    for route in &service.http.routes {
        out.push_str(&render_route_handler(route));
        out.push('\n');
    }
    let validation_helpers = validation::render_validation_helpers(service);
    if !validation_helpers.is_empty() {
        out.push_str(&validation_helpers);
        out.push('\n');
    }
    let needs_header_str = service.http.routes.iter().any(|r| !r.headers.is_empty())
        || service
            .sockets
            .sockets
            .iter()
            .any(|s| !s.headers.is_empty());
    let needs_header_parse = service
        .http
        .routes
        .iter()
        .flat_map(|r| r.headers.iter())
        .chain(
            service
                .sockets
                .sockets
                .iter()
                .flat_map(|s| s.headers.iter()),
        )
        .any(|h| matches!(h.ty, Type::Int | Type::Float));
    if needs_header_str {
        out.push_str(
            r#"
fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}
"#,
        );
        if needs_header_parse {
            out.push_str(
                r#"
fn header_parse<T: std::str::FromStr>(headers: &HeaderMap, name: &str) -> Option<T> {
    headers.get(name).and_then(|v| v.to_str().ok())?.parse().ok()
}
"#,
            );
        }
    }
    if service
        .http
        .routes
        .iter()
        .any(|r| r.call.service_base.is_none() && !r.call.is_async)
    {
        out.push_str(
            r#"
async fn call_blocking<T, F>(f: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .expect("blocking task failed")
}
"#,
        );
    }
    for route in &service.http.routes {
        if let Some(limit) = effective_rate_limit(route, service) {
            let name = format!("rate_limit_{}", handler_name(route));
            let static_name = format!("RATE_LIMIT_{}", handler_name(route).to_uppercase());
            out.push_str(&format!(
                r#"
static {static_name}: OnceLock<Mutex<(u32, Instant)>> = OnceLock::new();

async fn {name}<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {{
    let state = {static_name}.get_or_init(|| Mutex::new((0, Instant::now())));
    let mut guard = state.lock().unwrap();
    let elapsed = guard.1.elapsed();
    if elapsed >= Duration::from_secs({window}) {{
        *guard = (0, Instant::now());
    }}
    if guard.0 >= {max} {{
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }}
    guard.0 += 1;
    drop(guard);
    Ok(next.run(req).await)
}}
"#,
                name = name,
                static_name = static_name,
                max = limit.max,
                window = limit.per_seconds,
            ));
        }
    }
    out
}

pub(super) fn render_route_handler(route: &Route) -> String {
    let name = handler_name(route);
    let mut args = Vec::new();
    let needs_raw_headers = !route.headers.is_empty() || route.call.service_base.is_some();
    if needs_raw_headers {
        args.push("raw_headers: HeaderMap".to_string());
    }
    if !route.input.path.is_empty() {
        args.push(format!("Path(path): Path<{}>", path_struct_name(route)));
    }
    if !route.input.query.is_empty() {
        args.push(format!("Query(query): Query<{}>", query_struct_name(route)));
    }
    if !route.input.body.is_empty() {
        args.push(format!("Json(body): Json<{}>", body_struct_name(route)));
    }
    let args_sig = if args.is_empty() {
        "()".to_string()
    } else {
        format!("({})", args.join(", "))
    };

    let resp = super::util::primary_response(route);
    let status_expr = status_code_expr(resp.status);
    let header_struct = if route.headers.is_empty() {
        String::new()
    } else {
        render_headers_init(route, "raw_headers")
    };
    let validation_block = validation::render_validation_block(route);
    let return_expr = if route.healthcheck {
        healthcheck_return_expr(resp, &status_expr)
    } else if route.call.service_base.is_some() {
        let call = render_call_expr(route, needs_raw_headers);
        if resp.ty == Type::Void {
            format!(
                "    if let Err(s) = {call}.await {{\n        return s.into_response();\n    }}\n    {status}.into_response()\n",
                call = call,
                status = status_expr
            )
        } else {
            let result_line = format!(
                "    let result = match {call}.await {{\n        Ok(v) => v,\n        Err(s) => return s.into_response(),\n    }};\n",
                call = call
            );
            let body_expr = build_call_return_expr(resp, &status_expr, "result");
            format!(
                "{result_line}{body}\n",
                result_line = result_line,
                body = format!("{}{}", body_expr.trim_end(), ".into_response()")
            )
        }
    } else {
        let call_expr = render_call_expr(route, needs_raw_headers);
        build_call_return_expr(resp, &status_expr, &call_expr)
    };

    format!(
        r#"async fn {name}{args_sig} -> impl IntoResponse {{
{header_struct}{validation_block}    let response = {{
{return_expr}
    }};
    response.into_response()
}}
"#,
        name = name,
        args_sig = args_sig,
        header_struct = header_struct,
        validation_block = validation_block,
        return_expr = return_expr
    )
}

pub(super) fn render_input_structs(route: &Route) -> String {
    let mut out = String::new();
    if !route.input.path.is_empty() {
        out.push_str(&render_struct(
            &path_struct_name(route),
            &route.input.path,
            false,
        ));
    }
    if !route.input.query.is_empty() {
        out.push_str(&render_struct(
            &query_struct_name(route),
            &route.input.query,
            true,
        ));
    }
    if !route.input.body.is_empty() {
        out.push_str(&render_struct(
            &body_struct_name(route),
            &route.input.body,
            false,
        ));
    }
    if !route.headers.is_empty() {
        out.push_str(&render_struct(
            &headers_struct_name(route),
            &route.headers,
            false,
        ));
    }
    out
}

fn render_struct(name: &str, fields: &[Field], optional: bool) -> String {
    let mut out = String::new();
    out.push_str("#[derive(Deserialize)]\n");
    out.push_str(&format!("struct {} {{\n", name));
    for field in fields {
        let ty = rust_type(&field.ty);
        if optional || field.optional {
            out.push_str(&format!("    {}: Option<{}>,\n", field.name, ty));
        } else {
            out.push_str(&format!("    {}: {},\n", field.name, ty));
        }
    }
    out.push_str("}\n\n");
    out
}

fn path_struct_name(route: &Route) -> String {
    format!("{}Path", handler_struct_base(route))
}

fn query_struct_name(route: &Route) -> String {
    format!("{}Query", handler_struct_base(route))
}

fn body_struct_name(route: &Route) -> String {
    format!("{}Body", handler_struct_base(route))
}

fn headers_struct_name(route: &Route) -> String {
    format!("{}Headers", handler_struct_base(route))
}

fn handler_struct_base(route: &Route) -> String {
    let mut base = String::new();
    base.push_str(match route.method {
        crate::ir::HttpMethod::Get => "Get",
        crate::ir::HttpMethod::Post => "Post",
        crate::ir::HttpMethod::Put => "Put",
        crate::ir::HttpMethod::Patch => "Patch",
        crate::ir::HttpMethod::Delete => "Delete",
        crate::ir::HttpMethod::Options => "Options",
        crate::ir::HttpMethod::Head => "Head",
    });
    for ch in route.path.chars() {
        if ch.is_ascii_alphanumeric() {
            if base.ends_with('_') {
                base.pop();
            }
            base.push(ch.to_ascii_uppercase());
        } else if ch == '/' || ch == '-' || ch == '_' || ch == '{' || ch == '}' {
            if !base.ends_with('_') {
                base.push('_');
            }
        }
    }
    base.trim_matches('_').to_string()
}

fn build_call_return_expr(resp: &ResponseSpec, status_expr: &str, call_expr: &str) -> String {
    match resp.ty {
        Type::Void => format!("    {};\n    {}", call_expr, status_expr),
        Type::String => {
            if resp.status == 200 {
                format!("    {}", call_expr)
            } else {
                format!("    ({}, {})", status_expr, call_expr)
            }
        }
        _ => {
            let wrapped = format!("Json({})", call_expr);
            if resp.status == 200 {
                format!("    {}", wrapped)
            } else {
                format!("    ({}, {})", status_expr, wrapped)
            }
        }
    }
}

fn render_call_expr(route: &Route, needs_raw_headers: bool) -> String {
    if route.call.service_base.is_some() {
        let name = remote_call_fn_name(route);
        let args = render_call_args(&route.call.args);
        let headers_arg = if needs_raw_headers {
            "&raw_headers"
        } else {
            "&HeaderMap::new()"
        };
        return format!(
            "{name}({args}, {headers})",
            name = name,
            args = args,
            headers = headers_arg
        );
    }

    let module = &route.call.module;
    let func = to_snake_case(&route.call.function);
    let args = render_call_args(&route.call.args);
    if route.call.is_async {
        format!("{}::{}({}).await", module, func, args)
    } else {
        format!(
            "call_blocking(move || {}::{}({})).await",
            module, func, args
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
    let mut out = match input.source {
        InputSource::Path => "path".to_string(),
        InputSource::Query => "query".to_string(),
        InputSource::Body => "body".to_string(),
        InputSource::Headers => "headers".to_string(),
        InputSource::Input => "input".to_string(),
    };
    for seg in &input.path {
        out.push('.');
        out.push_str(seg);
    }
    out
}

fn healthcheck_return_expr(resp: &ResponseSpec, status_expr: &str) -> String {
    match resp.ty {
        Type::Void => format!("    {}", status_expr),
        Type::String => format!("    ({}, \"ok\")", status_expr),
        _ => format!("    {}", status_expr),
    }
}

fn render_headers_init(route: &Route, raw_headers: &str) -> String {
    let mut pre = Vec::new();
    let mut lines = Vec::new();
    for field in &route.headers {
        let key = header_runtime_key(&field.name);
        let expr = match field.ty {
            Type::String => format!("header_str(&{}, \"{}\")", raw_headers, key),
            Type::Int => format!("header_parse::<i64>(&{}, \"{}\")", raw_headers, key),
            Type::Float => format!("header_parse::<f64>(&{}, \"{}\")", raw_headers, key),
            Type::Bool => format!("header_parse::<bool>(&{}, \"{}\")", raw_headers, key),
            _ => format!("header_str(&{}, \"{}\")", raw_headers, key),
        };
        if field.optional {
            lines.push(format!("        {}: {},", field.name, expr));
        } else {
            let var = format!("header_{}", field.name);
            pre.push(format!("    let {} = {};", var, expr));
            let status = match missing_header_status(&field.name) {
                401 => "StatusCode::UNAUTHORIZED",
                _ => "StatusCode::BAD_REQUEST",
            };
            pre.push(format!(
                "    if {}.is_none() {{ return {}.into_response(); }}",
                var, status
            ));
            lines.push(format!("        {}: {}.unwrap(),", field.name, var));
        }
    }
    let mut out = String::new();
    if !pre.is_empty() {
        out.push_str(&pre.join("\n"));
        out.push('\n');
    }
    out.push_str("    let headers = ");
    out.push_str(&headers_struct_name(route));
    out.push_str(" {\n");
    out.push_str(&lines.join("\n"));
    out.push_str("\n    };\n");
    out
}

pub(super) fn route_middleware_chain(route: &Route, service: &Service) -> String {
    let mut names = Vec::new();
    if let Some(limit) = route
        .rate_limit
        .as_ref()
        .or(service.common.rate_limit.as_ref())
    {
        if limit.max > 0 {
            names.push(format!("rate_limit_{}", handler_name(route)));
        }
    }
    names.extend(route.middleware.iter().cloned());
    if let Some(auth) = effective_auth(route.auth.as_ref(), service.common.auth.as_ref()) {
        names.push(super::middleware::auth_middleware_name(auth));
    }

    let mut chain = String::new();
    for name in names {
        chain.push_str(&format!(".layer(from_fn({}))", name));
    }
    chain
}

fn effective_rate_limit<'a>(route: &'a Route, service: &'a Service) -> Option<&'a RateLimit> {
    route
        .rate_limit
        .as_ref()
        .or(service.common.rate_limit.as_ref())
}

pub(super) fn render_socket_call(call: &CallSpec) -> String {
    let module = &call.module;
    let func = &call.function;
    if call.is_async {
        format!("{module}::{func}(ctx.clone(), frame.data).await")
    } else {
        format!("{module}::{func}(ctx.clone(), frame.data)")
    }
}
