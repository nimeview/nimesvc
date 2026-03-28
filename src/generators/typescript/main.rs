use crate::generators::common::headers::{default_cors_headers, default_cors_methods};
use crate::ir::{EventsBroker, Service, UseScope};

use super::routes::{render_env_checks, render_rate_limit_helpers, render_routes};
use super::sockets::render_socket_helpers;
use super::util::{needs_middleware, needs_remote_calls, render_module_imports};

pub(super) fn build_main_ts(service: &Service) -> String {
    let mut imports = vec![
        "import express, { Request, Response } from 'express';".to_string(),
        "import * as Types from './types';".to_string(),
    ];
    let needs_event_consumers = matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    ) && !service.events.subscribes.is_empty();
    let needs_ws = !service.sockets.sockets.is_empty();
    if needs_ws {
        imports.push("import { WebSocketServer, WebSocket } from 'ws';".to_string());
    }
    if !service.common.env.is_empty() {
        imports.push("import process from 'process';".to_string());
    }
    if needs_middleware(service) {
        imports.push("import * as middleware from './middleware';".to_string());
    }
    if needs_remote_calls(service) {
        imports.push("import * as remoteCalls from './remote_calls';".to_string());
    }
    if needs_event_consumers {
        imports.push("import * as events from './events';".to_string());
    }
    imports.extend(render_module_imports(service, UseScope::Runtime));

    let routes = render_routes(service);
    let rate_helpers = render_rate_limit_helpers(service);
    let socket_helpers = render_socket_helpers(service);
    let env_checks = render_env_checks(service);
    let access_log_block = render_access_log_block(service);

    let address = service.common.address.as_deref().unwrap_or("127.0.0.1");
    let port = service.common.port.unwrap_or(3000);
    let cors_block = render_cors_block(service);
    let event_consumers = if needs_event_consumers {
        "events.startEventConsumers();\n"
    } else {
        ""
    };
    format!(
        r#"{imports}

{env_checks}const app = express();
app.use(express.json());
{access_log_block}
{cors_block}

{rate_helpers}{routes}
const server = app.listen({port}, "{address}", () => {{
  console.log('Server running on http://{address}:{port}');
}});
{socket_helpers}
{event_consumers}
"#,
        imports = imports.join("\n"),
        routes = routes,
        rate_helpers = rate_helpers,
        address = address,
        port = port,
        env_checks = env_checks,
        access_log_block = access_log_block,
        socket_helpers = socket_helpers,
        event_consumers = event_consumers,
        cors_block = cors_block
    )
}

fn render_access_log_block(service: &Service) -> String {
    if service.http.routes.is_empty() {
        return String::new();
    }
    format!(
        r#"app.use((req, res, next) => {{
  const started = Date.now();
  res.on('finish', () => {{
    console.error('[{service}] ' + req.method + ' ' + req.originalUrl + ' -> ' + res.statusCode + ' (' + (Date.now() - started) + 'ms)');
  }});
  next();
}});

"#,
        service = service.name
    )
}

fn render_cors_block(service: &Service) -> String {
    let Some(cors) = &service.common.cors else {
        return String::new();
    };
    let allow_any = if cors.allow_any { "true" } else { "false" };
    let list = if cors.allow_any {
        "new Set<string>()".to_string()
    } else {
        let items = cors
            .origins
            .iter()
            .map(|o| format!("\"{}\"", o.replace('\\', "\\\\").replace('\"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", ");
        format!("new Set<string>([{}])", items)
    };
    let methods = if cors.methods.is_empty() {
        default_cors_methods().to_string()
    } else {
        cors.methods.join(",")
    };
    let headers = if cors.headers.is_empty() {
        default_cors_headers().to_string()
    } else {
        cors.headers.join(",")
    };
    format!(
        r#"const corsAllowAny = {allow_any};
const corsAllowList = {list};
app.use((req, res, next) => {{
  const origin = req.headers.origin as string | undefined;
  if (corsAllowAny) {{
    res.setHeader('Access-Control-Allow-Origin', '*');
  }} else if (origin && corsAllowList.has(origin)) {{
    res.setHeader('Access-Control-Allow-Origin', origin);
    res.setHeader('Vary', 'Origin');
  }}
  res.setHeader('Access-Control-Allow-Headers', '{headers}');
  res.setHeader('Access-Control-Allow-Methods', '{methods}');
  if (req.method === 'OPTIONS') {{
    res.status(204).end();
    return;
  }}
  next();
}});

"#,
        allow_any = allow_any,
        list = list,
        methods = methods,
        headers = headers
    )
}
