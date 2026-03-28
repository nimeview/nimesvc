use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::generators::rpc;
use crate::generators::typescript::events::build_events_ts;
use crate::generators::typescript::types::build_types_ts;
use crate::generators::typescript::util::render_module_imports;
use crate::ir::{AuthSpec, EventsBroker, RateLimit, Service, Type, UseScope};

pub(super) fn generate(service: &Service, out_dir: &Path) -> Result<()> {
    let src_dir = out_dir.join("src");
    fs::create_dir_all(&src_dir)
        .with_context(|| format!("Failed to create '{}'", src_dir.display()))?;

    let proto = rpc::build_proto(service);
    fs::write(out_dir.join("rpc.proto"), proto).with_context(|| "Failed to write rpc.proto")?;

    let types_ts = build_types_ts(service);
    fs::write(src_dir.join("types.ts"), types_ts)
        .with_context(|| "Failed to write src/types.ts")?;
    if !service.events.definitions.is_empty() {
        let events_ts = build_events_ts(service);
        fs::write(src_dir.join("events.ts"), events_ts)
            .with_context(|| "Failed to write src/events.ts")?;
    }

    let package_json = build_package_json(service);
    fs::write(out_dir.join("package.json"), package_json)
        .with_context(|| "Failed to write package.json")?;

    let tsconfig = r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "CommonJS",
    "outDir": "dist",
    "esModuleInterop": true,
    "strict": true,
    "lib": ["ES2020", "DOM"]
  }
}
"#;
    fs::write(out_dir.join("tsconfig.json"), tsconfig)
        .with_context(|| "Failed to write tsconfig.json")?;

    let index_ts = build_index_ts(service);
    fs::write(src_dir.join("index.ts"), index_ts)
        .with_context(|| "Failed to write src/index.ts")?;

    Ok(())
}

fn build_package_json(service: &Service) -> String {
    let mut deps = vec![
        "@grpc/grpc-js\": \"^1.10.4\"".to_string(),
        "@grpc/proto-loader\": \"^0.7.13\"".to_string(),
    ];
    if matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    ) {
        deps.push("redis\": \"^4.6.13\"".to_string());
    }
    for module in &service.common.modules {
        if module.path.is_none() {
            let name = module.name.split("::").next().unwrap().to_string();
            if deps.iter().any(|d| d.starts_with(&format!("{name}\""))) {
                continue;
            }
            let version = module.version.clone().unwrap_or_else(|| "*".to_string());
            deps.push(format!("{}\": \"{}\"", name, version));
        }
    }
    let deps_block = deps.join(",\n    \"");
    format!(
        r#"{{
  "name": "{name}-grpc",
  "version": "0.1.0",
  "type": "commonjs",
  "scripts": {{
    "install:deps": "bun install",
    "dev": "ts-node src/index.ts",
    "build": "tsc",
    "start": "node dist/index.js"
  }},
  "dependencies": {{
    "{deps_block}
  }},
  "devDependencies": {{
    "typescript": "^5.5.0",
    "ts-node": "^10.9.2"
  }}
}}
"#,
        name = service.name.to_lowercase(),
        deps_block = deps_block
    )
}

fn build_index_ts(service: &Service) -> String {
    let mut imports = vec![
        "import * as grpc from '@grpc/grpc-js';".to_string(),
        "import * as protoLoader from '@grpc/proto-loader';".to_string(),
        "import path from 'path';".to_string(),
    ];
    let needs_tls = service
        .rpc
        .grpc_config
        .as_ref()
        .and_then(|c| c.tls_cert.as_ref())
        .is_some();
    if needs_tls {
        imports.push("import fs from 'fs';".to_string());
    }
    imports.extend(render_module_imports(service, UseScope::Runtime));
    let needs_event_consumers = matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    ) && !service.events.subscribes.is_empty();
    if needs_event_consumers {
        imports.push("import * as events from './events';".to_string());
    }

    let pkg = format!("nimesvc.{}", service.name.to_lowercase());
    let service_name = service.name.clone();

    let mut methods = String::new();
    for rpc in &service.rpc.methods {
        let method = rpc.name.clone();
        let call_expr = render_call_expr(rpc);
        let response = render_response_expr(rpc);
        let auth = render_auth_check(rpc, service);
        let headers_init = render_headers_init(rpc);
        let middleware = render_middleware_calls(rpc, service);
        let rate_limit = render_rate_limit_check(rpc, service);
        methods.push_str(&format!(
            "  {method}: async (call: any, callback: any) => {{\n    const started = Date.now();\n    try {{\n      const metadata = call.metadata;\n{auth}{headers_init}{middleware}{rate_limit}      const input = call.request;\n      const result = {call_expr};\n      console.error('[{service_name}] gRPC {rpc_name} -> ' + grpcStatusLabel() + ' (' + (Date.now() - started) + 'ms)');\n      callback(null, {response});\n    }} catch (err: any) {{\n      console.error('[{service_name}] gRPC {rpc_name} -> ' + grpcStatusLabel(err) + ' (' + (Date.now() - started) + 'ms)');\n      callback(err);\n    }}\n  }},\n",
            method = method,
            call_expr = call_expr,
            response = response,
            auth = auth,
            headers_init = headers_init,
            middleware = middleware,
            rate_limit = rate_limit,
            service_name = service.name,
            rpc_name = rpc.code_name(),
        ));
    }

    let (addr, port) = grpc_addr_port(service);
    let server_opts = render_server_options(service);
    let creds = render_server_creds(service);
    let headers_helpers = render_header_helpers(service);
    let access_log_helpers = render_access_log_helpers(service);
    let middleware_defs = build_middleware_ts(service);
    let rate_limit_defs = render_rate_limit_defs(service);

    let event_consumers = if needs_event_consumers {
        "events.startEventConsumers();\n"
    } else {
        ""
    };
    format!(
        r#"{imports}

const protoPath = path.join(__dirname, '..', 'rpc.proto');
const packageDef = protoLoader.loadSync(protoPath, {{
  keepCase: true,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
}});
const proto = (grpc.loadPackageDefinition(packageDef) as any);

const serviceDef = proto.{pkg}.{service_name};
{server_opts}
const server = new grpc.Server(serverOptions);

server.addService(serviceDef.service, {{
{methods}}});

server.bindAsync('{addr}:{port}', {creds}, () => {{
  server.start();
  console.log('gRPC server listening on {addr}:{port}');
{event_consumers}
}});
{headers_helpers}
{access_log_helpers}
{middleware_defs}
{rate_limit_defs}
"#,
        imports = imports.join("\n"),
        pkg = pkg,
        service_name = service_name,
        methods = methods,
        addr = addr,
        port = port,
        server_opts = server_opts,
        creds = creds,
        headers_helpers = headers_helpers,
        access_log_helpers = access_log_helpers,
        middleware_defs = middleware_defs,
        rate_limit_defs = rate_limit_defs,
        event_consumers = event_consumers,
    )
}

fn render_call_expr(rpc: &crate::ir::RpcDef) -> String {
    let module = &rpc.call.module;
    let func = &rpc.call.function;
    let args = render_call_args(&rpc.call.args);
    format!("{}.{func}({args})", module, func = func, args = args)
}

fn render_call_args(args: &[crate::ir::CallArg]) -> String {
    let mut out = Vec::new();
    for arg in args {
        out.push(render_input_ref(&arg.value));
    }
    out.join(", ")
}

fn render_input_ref(input: &crate::ir::InputRef) -> String {
    if input.source == crate::ir::InputSource::Headers {
        if input.path.is_empty() {
            return "headers".to_string();
        }
        return format!("headers.{}", input.path.join("."));
    }
    if input.path.is_empty() {
        return "input".to_string();
    }
    format!("input.{}", input.path.join("."))
}

fn render_response_expr(rpc: &crate::ir::RpcDef) -> String {
    use crate::ir::Type;
    match &rpc.output {
        Type::Void => "{}".to_string(),
        Type::Named(_) | Type::Object(_) => "result".to_string(),
        Type::Any | Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) => {
            "result".to_string()
        }
        Type::Array(_) | Type::String | Type::Int | Type::Float | Type::Bool => {
            "{ value: result }".to_string()
        }
    }
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

fn render_server_options(service: &Service) -> String {
    let mut lines = Vec::new();
    lines.push("const serverOptions: Record<string, any> = {};".to_string());
    if let Some(cfg) = &service.rpc.grpc_config {
        if let Some(size) = cfg.max_message_size {
            lines.push(format!(
                "serverOptions['grpc.max_receive_message_length'] = {size};"
            ));
            lines.push(format!(
                "serverOptions['grpc.max_send_message_length'] = {size};"
            ));
        }
    }
    lines.join("\n")
}

fn render_server_creds(service: &Service) -> String {
    if let Some(cfg) = &service.rpc.grpc_config {
        if let (Some(cert), Some(key)) = (cfg.tls_cert.as_ref(), cfg.tls_key.as_ref()) {
            return format!(
                "grpc.ServerCredentials.createSsl(null, [{{ cert_chain: fs.readFileSync('{cert}'), private_key: fs.readFileSync('{key}') }}])",
                cert = cert,
                key = key
            );
        }
    }
    "grpc.ServerCredentials.createInsecure()".to_string()
}

fn render_header_helpers(service: &Service) -> String {
    if !service.rpc.methods.iter().any(|r| !r.headers.is_empty())
        && service.common.auth.is_none()
        && service.rpc.methods.iter().all(|r| r.auth.is_none())
    {
        return String::new();
    }
    r#"
function metaGet(metadata: any, name: string): string | undefined {
  const vals = metadata.get(name);
  if (!vals || vals.length === 0) return undefined;
  return String(vals[0]);
}
"#
    .to_string()
}

fn render_access_log_helpers(service: &Service) -> String {
    if service.rpc.methods.is_empty() {
        return String::new();
    }
    r#"
function grpcStatusLabel(err?: any): string {
  if (!err || err.code === undefined || err.code === null) return 'OK';
  for (const [name, value] of Object.entries(grpc.status)) {
    if (value === err.code) return String(name);
  }
  return String(err.code);
}
"#
    .to_string()
}

fn render_headers_init(rpc: &crate::ir::RpcDef) -> String {
    if rpc.headers.is_empty() {
        return String::new();
    }
    let mut lines = Vec::new();
    for field in &rpc.headers {
        let getter = format!("metaGet(metadata, \"{}\")", field.name);
        let value = match field.ty {
            Type::Int => format!("{getter} ? Number({getter}) : undefined", getter = getter),
            Type::Float => format!("{getter} ? Number({getter}) : undefined", getter = getter),
            Type::Bool => format!(
                "{getter} ? ({getter} === 'true') : undefined",
                getter = getter
            ),
            _ => format!("{getter} ?? undefined", getter = getter),
        };
        if field.optional {
            lines.push(format!(
                "    const {name} = {value};",
                name = field.name,
                value = value
            ));
        } else {
            lines.push(format!(
                "    const {name} = {value};\n    if ({name} === undefined) {{\n      return callback({{ code: grpc.status.INVALID_ARGUMENT, message: 'missing header {header}' }});\n    }}",
                name = field.name,
                value = value,
                header = field.name
            ));
        }
    }
    let mut out = String::new();
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("    const headers = { ");
    out.push_str(
        &rpc.headers
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str(" };\n");
    out
}

fn effective_auth<'a>(rpc: &'a crate::ir::RpcDef, service: &'a Service) -> Option<&'a AuthSpec> {
    match rpc.auth.as_ref() {
        Some(AuthSpec::None) => None,
        Some(auth) => Some(auth),
        None => match service.common.auth.as_ref() {
            Some(AuthSpec::None) => None,
            other => other,
        },
    }
}

fn render_auth_check(rpc: &crate::ir::RpcDef, service: &Service) -> String {
    let Some(auth) = effective_auth(rpc, service) else {
        return String::new();
    };
    match auth {
        AuthSpec::Bearer => {
            "      const token = metaGet(metadata, 'authorization');\n      if (!token) { return callback({ code: grpc.status.UNAUTHENTICATED, message: 'missing authorization' }); }\n".to_string()
        }
        AuthSpec::ApiKey => {
            "      const token = metaGet(metadata, 'x-api-key');\n      if (!token) { return callback({ code: grpc.status.UNAUTHENTICATED, message: 'missing api key' }); }\n".to_string()
        }
        AuthSpec::None => String::new(),
    }
}

fn render_middleware_calls(rpc: &crate::ir::RpcDef, service: &Service) -> String {
    let mut out = String::new();
    for name in service
        .common
        .middleware
        .iter()
        .chain(rpc.middleware.iter())
    {
        out.push_str(&format!(
            "      await {name}({{ metadata }});\n",
            name = name
        ));
    }
    out
}

fn effective_rate_limit<'a>(
    rpc: &'a crate::ir::RpcDef,
    service: &'a Service,
) -> Option<&'a RateLimit> {
    rpc.rate_limit
        .as_ref()
        .or(service.common.rate_limit.as_ref())
}

fn render_rate_limit_check(rpc: &crate::ir::RpcDef, service: &Service) -> String {
    if effective_rate_limit(rpc, service).is_none() {
        return String::new();
    };
    let name = format!("rateLimit{}", rpc.code_name());
    format!(
        "      if (!{name}.allow()) {{ return callback({{ code: grpc.status.RESOURCE_EXHAUSTED, message: 'rate limited' }}); }}\n",
        name = name
    )
}

fn build_middleware_ts(service: &Service) -> String {
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
    out.push_str("\nexport type RpcContext = { metadata: any };\n");
    out.push_str(
        "function middlewareDirective(ctx: RpcContext, name: string): string | undefined {\n",
    );
    out.push_str("  return metaGet(ctx.metadata, `x-nimesvc-middleware-${name.replace(/_/g, '-')}`)?.trim().toLowerCase();\n");
    out.push_str("}\n");
    for name in names {
        out.push_str(&format!(
            "export async function {name}(ctx: RpcContext): Promise<void> {{\n  const directive = middlewareDirective(ctx, '{name}');\n  if (directive === 'block' || directive === 'deny' || directive === 'forbid') {{\n    const err: any = new Error('blocked by middleware {name}');\n    err.code = grpc.status.PERMISSION_DENIED;\n    throw err;\n  }}\n}}\n",
            name = name
        ));
    }
    out
}

fn render_rate_limit_defs(service: &Service) -> String {
    let mut entries = Vec::new();
    for rpc in &service.rpc.methods {
        if let Some(limit) = effective_rate_limit(rpc, service) {
            entries.push(format!(
                "const rateLimit{} = new RateLimit({}, {} * 1000);",
                rpc.code_name(),
                limit.max,
                limit.per_seconds
            ));
        }
    }
    if entries.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(
        r#"
class RateLimit {
  private count = 0;
  private reset = Date.now();
  constructor(private max: number, private windowMs: number) {}
  allow(): boolean {
    const now = Date.now();
    if (now - this.reset >= this.windowMs) {
      this.reset = now;
      this.count = 0;
    }
    if (this.count >= this.max) return false;
    this.count += 1;
    return true;
  }
}
"#,
    );
    out.push('\n');
    out.push_str(&entries.join("\n"));
    out.push('\n');
    out
}
