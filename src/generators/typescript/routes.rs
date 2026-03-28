use crate::generators::common::env::render_env_checks_ts;
use crate::ir::{CallArg, InputRef, InputSource, Route, Service, Type, effective_auth};

use super::util::{
    auth_middleware_name, effective_rate_limit, express_path, handler_struct_base, method_name,
    primary_response, rate_limit_name, remote_call_fn_name,
};

pub(super) fn render_routes(service: &Service) -> String {
    let mut routes = String::new();
    for route in &service.http.routes {
        routes.push_str(&render_route(route, service));
        routes.push('\n');
    }
    routes
}

pub(super) fn render_env_checks(service: &Service) -> String {
    render_env_checks_ts(service)
}

pub(super) fn render_rate_limit_helpers(service: &Service) -> String {
    let mut out = String::new();
    let mut any = false;
    for route in &service.http.routes {
        if effective_rate_limit(route, service).is_some() {
            any = true;
        }
    }
    if !any {
        return out;
    }
    out.push_str("const _rateLimits = new Map<string, { count: number; reset: number }>();\n\n");
    for route in &service.http.routes {
        if let Some(limit) = effective_rate_limit(route, service) {
            let name = rate_limit_name(route);
            out.push_str(&format!(
                "async function {name}(req: Request, res: Response, next: Function) {{\n  const key = \"{name}\";\n  const now = Date.now();\n  const windowMs = {window} * 1000;\n  let state = _rateLimits.get(key);\n  if (!state || now - state.reset >= windowMs) {{\n    _rateLimits.set(key, {{ count: 1, reset: now }});\n    return next();\n  }}\n  if (state.count >= {max}) {{\n    return res.status(429).send(\"Too Many Requests\");\n  }}\n  state.count += 1;\n  return next();\n}}\n\n",
                name = name,
                window = limit.per_seconds,
                max = limit.max
            ));
        }
    }
    out
}

fn render_route(route: &Route, service: &Service) -> String {
    let method = method_name(route.method.clone());
    let path = express_path(&route.path);
    let mut arg_lines = Vec::new();
    if !route.input.path.is_empty() {
        arg_lines.push(format!(
            "  const path = Types.parsePath{}(req);",
            handler_struct_base(route)
        ));
    }
    if !route.input.query.is_empty() {
        arg_lines.push(format!(
            "  const query = Types.parseQuery{}(req);",
            handler_struct_base(route)
        ));
    }
    if !route.input.body.is_empty() {
        arg_lines.push(format!(
            "  const body = Types.parseBody{}(req);",
            handler_struct_base(route)
        ));
    }
    if !route.headers.is_empty() {
        arg_lines.push(format!(
            "  const headers = Types.parseHeaders{}(req);",
            handler_struct_base(route)
        ));
    }
    let resp = primary_response(route);
    let response_send = if resp.ty == Type::Void {
        format!("  res.sendStatus({});", resp.status)
    } else {
        format!("  res.status({}).json(result);", resp.status)
    };
    let middleware_chain = build_middleware_chain(route, service);
    let call_expr = render_call_expr(route);
    let call_line = if route.call.service_base.is_some() || route.call.is_async {
        format!("  const result = await {call_expr};", call_expr = call_expr)
    } else {
        format!("  const result = {call_expr};", call_expr = call_expr)
    };
    let health_send = if resp.ty == Type::Void {
        format!("  res.sendStatus({});", resp.status)
    } else {
        format!("  res.status({}).send(\"ok\");", resp.status)
    };
    format!(
        r#"app.{method}("{path}", {middleware_chain}async (req: Request, res: Response) => {{
  try {{
{arg_lines}
{handler_body}
  }} catch (err: any) {{
    const status = typeof err?.status === "number" ? err.status : 400;
    res.status(status).json({{ error: String(err?.message || err) }});
  }}
}});
"#,
        method = method,
        path = path,
        handler_body = if route.healthcheck {
            health_send
        } else {
            format!(
                "{call_line}\n{response_send}",
                call_line = call_line,
                response_send = response_send
            )
        },
        arg_lines = if arg_lines.is_empty() {
            String::new()
        } else {
            arg_lines.join("\n")
        },
        middleware_chain = middleware_chain
    )
}

fn render_call_expr(route: &Route) -> String {
    if route.call.service_base.is_some() {
        let name = remote_call_fn_name(route);
        let args = render_remote_args(&route.call.args);
        return format!("remoteCalls.{name}({args}, req)", name = name, args = args);
    }
    let module = &route.call.module;
    let func = &route.call.function;
    let args = render_call_args(&route.call.args);
    format!("{}.{func}({args})", module, func = func, args = args)
}

fn render_call_args(args: &[CallArg]) -> String {
    let mut out = Vec::new();
    for arg in args {
        out.push(render_input_ref(&arg.value));
    }
    out.join(", ")
}

fn render_remote_args(args: &[CallArg]) -> String {
    let mut out = Vec::new();
    for (idx, arg) in args.iter().enumerate() {
        let name = arg.name.clone().unwrap_or_else(|| format!("arg{}", idx));
        let value = render_input_ref(&arg.value);
        out.push(format!("{name}: {value}", name = name, value = value));
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
        format!("{}.{}", base, input.path.join("."))
    }
}

fn build_middleware_chain(route: &Route, service: &Service) -> String {
    let mut names = Vec::new();
    if effective_rate_limit(route, service).is_some() {
        names.push(rate_limit_name(route));
    }
    names.extend(service.common.middleware.iter().cloned());
    names.extend(route.middleware.iter().cloned());
    if let Some(auth) = effective_auth(route.auth.as_ref(), service.common.auth.as_ref()) {
        names.push(auth_middleware_name(auth));
    }
    if names.is_empty() {
        return String::new();
    }
    let chain = names
        .into_iter()
        .map(|n| {
            if n.starts_with("rate_limit_") {
                n
            } else {
                format!("middleware.{}", n)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}, ", chain)
}
