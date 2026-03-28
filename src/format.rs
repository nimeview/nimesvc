use crate::ir::*;

pub fn format_project(project: &Project) -> String {
    let mut out = String::new();
    if let Some(version) = project.common.default_version {
        out.push_str(&format!("version {}\n\n", version));
    }
    if let Some(output) = &project.common.output {
        out.push_str(&format!("output \"{}\"\n\n", output));
    }
    if let Some(auth) = &project.common.auth {
        out.push_str(&format!("auth {}\n", format_auth(auth)));
    }
    for mw in &project.common.middleware {
        out.push_str(&format!("middleware {}\n", mw));
    }
    if project.common.auth.is_some() || !project.common.middleware.is_empty() {
        out.push('\n');
    }
    for module in &project.common.modules {
        out.push_str(&format!("{}\n", format_use(module)));
    }
    if !project.common.modules.is_empty() {
        out.push('\n');
    }

    for (idx, service) in project.services.iter().enumerate() {
        out.push_str(&format_service(service));
        if idx + 1 < project.services.len() {
            out.push('\n');
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn format_service(service: &Service) -> String {
    let mut out = String::new();
    let mut svc_decl = format!("service {}", service.name);
    if let Some(lang) = &service.language {
        svc_decl.push(' ');
        svc_decl.push_str(match lang {
            Lang::Rust => "rust",
            Lang::TypeScript => "ts",
            Lang::Go => "go",
        });
    }
    svc_decl.push(':');
    out.push_str(&svc_decl);
    out.push('\n');

    if service.common.address.is_some()
        || service.common.port.is_some()
        || service.common.base_url.is_some()
        || service.common.cors.is_some()
    {
        out.push_str("    config:\n");
        if let Some(address) = &service.common.address {
            out.push_str(&format!("        address: \"{}\"\n", address));
        }
        if let Some(port) = &service.common.port {
            out.push_str(&format!("        port: {}\n", port));
        }
        if let Some(base_url) = &service.common.base_url {
            out.push_str(&format!("        base_url: \"{}\"\n", base_url));
        }
        if let Some(cors) = &service.common.cors {
            if cors.allow_any {
                out.push_str("        cors: \"*\"\n");
            } else if !cors.origins.is_empty() {
                out.push_str(&format!("        cors: \"{}\"\n", cors.origins.join(", ")));
            }
            if !cors.methods.is_empty() {
                out.push_str(&format!(
                    "        cors_methods: \"{}\"\n",
                    cors.methods.join(", ")
                ));
            }
            if !cors.headers.is_empty() {
                out.push_str(&format!(
                    "        cors_headers: \"{}\"\n",
                    cors.headers.join(", ")
                ));
            }
        }
    }

    if let Some(cfg) = &service.rpc.grpc_config {
        out.push_str("    grpc_config:\n");
        if let Some(address) = &cfg.address {
            out.push_str(&format!("        address: \"{}\"\n", address));
        }
        if let Some(port) = &cfg.port {
            out.push_str(&format!("        port: {}\n", port));
        }
        if let Some(size) = &cfg.max_message_size {
            out.push_str(&format!("        max_message_size: {}\n", size));
        }
        if let (Some(cert), Some(key)) = (&cfg.tls_cert, &cfg.tls_key) {
            out.push_str(&format!("        tls: \"{}\" \"{}\"\n", cert, key));
        }
    }

    if let Some(cfg) = &service.events.config {
        out.push_str("    events_config:\n");
        out.push_str(&format!(
            "        broker: \"{}\"\n",
            match cfg.broker {
                EventsBroker::Redis => "redis",
            }
        ));
        if let Some(url) = &cfg.url {
            out.push_str(&format!("        url: \"{}\"\n", url));
        }
        if let Some(prefix) = &cfg.stream_prefix {
            out.push_str(&format!("        stream_prefix: \"{}\"\n", prefix));
        }
        if let Some(group) = &cfg.group {
            out.push_str(&format!("        group: \"{}\"\n", group));
        }
        if let Some(consumer) = &cfg.consumer {
            out.push_str(&format!("        consumer: \"{}\"\n", consumer));
        }
    }

    for module in &service.common.modules {
        out.push_str(&format!("    {}\n", format_use(module)));
    }

    for env in &service.common.env {
        if let Some(default) = &env.default {
            out.push_str(&format!("    env {}=\"{}\"\n", env.name, default));
        } else {
            out.push_str(&format!("    env {}\n", env.name));
        }
    }

    if let Some(auth) = &service.common.auth {
        out.push_str(&format!("    auth {}\n", format_auth(auth)));
    }

    for mw in &service.common.middleware {
        out.push_str(&format!("    middleware {}\n", mw));
    }

    if let Some(limit) = &service.common.rate_limit {
        out.push_str(&format!("    rate_limit {}\n", format_rate_limit(limit)));
    }

    if !service.http.headers.is_empty() {
        out.push_str("    headers:\n");
        for field in &service.http.headers {
            out.push_str(&format!("        {}\n", format_field(field)));
        }
    }

    for ev in &service.events.emits {
        out.push_str(&format!("    emit {}\n", ev.display()));
    }
    for ev in &service.events.subscribes {
        out.push_str(&format!("    subscribe {}\n", ev.display()));
    }

    for ty in &service.schema.types {
        out.push_str(&format!("\n    type {}:\n", ty.name.display()));
        for field in &ty.fields {
            out.push_str(&format!("        {}\n", format_field(field)));
        }
    }

    for en in &service.schema.enums {
        out.push_str(&format!("\n    enum {}:\n", en.name.display()));
        for var in &en.variants {
            if let Some(val) = var.value {
                out.push_str(&format!("        {} = {}\n", var.name, val));
            } else {
                out.push_str(&format!("        {}\n", var.name));
            }
        }
    }

    for ev in &service.events.definitions {
        out.push_str(&format!("\n    event {}:\n", ev.name.display()));
        out.push_str(&format!("        payload: {}\n", format_type(&ev.payload)));
    }

    for rpc in &service.rpc.methods {
        out.push_str(&format!(
            "\n    rpc {}.{}:\n",
            rpc.service,
            format_name_version(&rpc.name, rpc.version)
        ));
        for module in &rpc.modules {
            out.push_str(&format!("        {}\n", format_use(module)));
        }
        if !rpc.input.is_empty() {
            out.push_str("        input:\n");
            for field in &rpc.input {
                out.push_str(&format!("            {}\n", format_field(field)));
            }
        }
        if !rpc.headers.is_empty() {
            out.push_str("        headers:\n");
            for field in &rpc.headers {
                out.push_str(&format!("            {}\n", format_field(field)));
            }
        }
        out.push_str(&format!("        output {}\n", format_type(&rpc.output)));
        if let Some(auth) = &rpc.auth {
            out.push_str(&format!("        auth {}\n", format_auth(auth)));
        }
        for mw in &rpc.middleware {
            out.push_str(&format!("        middleware {}\n", mw));
        }
        if let Some(limit) = &rpc.rate_limit {
            out.push_str(&format!(
                "        rate_limit {}\n",
                format_rate_limit(limit)
            ));
        }
        out.push_str(&format!("        call {}\n", format_call(&rpc.call)));
    }

    for socket in &service.sockets.sockets {
        out.push_str(&format!(
            "\n    socket {} \"{}\":\n",
            socket.name.display(),
            socket.path
        ));
        for room in &socket.rooms {
            out.push_str(&format!("        room \"{}\"\n", room));
        }
        for trigger in &socket.triggers {
            out.push_str(&format!("        trigger {}:\n", trigger.name.display()));
            if let Some(room) = &trigger.room {
                out.push_str(&format!("            room \"{}\"\n", room));
            }
            out.push_str(&format!(
                "            payload {}\n",
                format_type(&trigger.payload)
            ));
        }
        for module in &socket.modules {
            out.push_str(&format!("        {}\n", format_use(module)));
        }
        if let Some(auth) = &socket.auth {
            out.push_str(&format!("        auth {}\n", format_auth(auth)));
        }
        for mw in &socket.middleware {
            out.push_str(&format!("        middleware {}\n", mw));
        }
        if let Some(limit) = &socket.rate_limit {
            out.push_str(&format!(
                "        rate_limit {}\n",
                format_rate_limit(limit)
            ));
        }
        if !socket.headers.is_empty() {
            out.push_str("        headers:\n");
            for field in &socket.headers {
                out.push_str(&format!("            {}\n", format_field(field)));
            }
        }
        if !socket.inbound.is_empty() {
            out.push_str("        inbound:\n");
            for msg in &socket.inbound {
                out.push_str(&format!(
                    "            {} -> {}\n",
                    msg.name.display(),
                    format_call(&msg.handler)
                ));
            }
        }
        if !socket.outbound.is_empty() {
            out.push_str("        outbound:\n");
            for msg in &socket.outbound {
                out.push_str(&format!(
                    "            {} -> {}\n",
                    msg.name.display(),
                    format_call(&msg.handler)
                ));
            }
        }
    }

    for route in &service.http.routes {
        out.push_str(&format!("\n    {} \"{}\":\n", route.method, route.path));
        if !route.input.path.is_empty()
            || !route.input.query.is_empty()
            || !route.input.body.is_empty()
        {
            out.push_str("        input:\n");
            if !route.input.path.is_empty() {
                out.push_str("            path:\n");
                for field in &route.input.path {
                    out.push_str(&format!("                {}\n", format_field(field)));
                }
            }
            if !route.input.query.is_empty() {
                out.push_str("            query:\n");
                for field in &route.input.query {
                    out.push_str(&format!("                {}\n", format_field(field)));
                }
            }
            if !route.input.body.is_empty() {
                out.push_str("            body:\n");
                for field in &route.input.body {
                    out.push_str(&format!("                {}\n", format_field(field)));
                }
            }
        }
        if !route.headers.is_empty() {
            out.push_str("        headers:\n");
            for field in &route.headers {
                out.push_str(&format!("            {}\n", format_field(field)));
            }
        }
        if route.responses.len() == 1 {
            let resp = &route.responses[0];
            out.push_str(&format!("        response {}\n", format_response(resp)));
        } else if !route.responses.is_empty() {
            out.push_str("        responses:\n");
            for resp in &route.responses {
                out.push_str(&format!("            {}\n", format_response(resp)));
            }
        }
        if let Some(auth) = &route.auth {
            out.push_str(&format!("        auth {}\n", format_auth(auth)));
        }
        for mw in &route.middleware {
            out.push_str(&format!("        middleware {}\n", mw));
        }
        if let Some(limit) = &route.rate_limit {
            out.push_str(&format!(
                "        rate_limit {}\n",
                format_rate_limit(limit)
            ));
        }
        if route.healthcheck {
            out.push_str("        healthcheck\n");
        }
        out.push_str(&format!("        call {}\n", format_call(&route.call)));
    }

    out
}

fn format_use(module: &ModuleUse) -> String {
    let mut out = String::new();
    out.push_str("use ");
    match module.scope {
        UseScope::Runtime => out.push_str("runtime "),
        UseScope::Compile => out.push_str("compile "),
        UseScope::Both => {}
    }
    out.push_str(&module.name);
    if let Some(path) = &module.path {
        out.push_str(&format!(" \"{}\"", path));
    } else if let Some(version) = &module.version {
        out.push_str(&format!(" \"{}\"", version));
    }
    if let Some(alias) = &module.alias {
        out.push_str(&format!(" as {}", alias));
    }
    out
}

fn format_auth(auth: &AuthSpec) -> &'static str {
    match auth {
        AuthSpec::None => "none",
        AuthSpec::Bearer => "bearer",
        AuthSpec::ApiKey => "api_key",
    }
}

fn format_rate_limit(limit: &RateLimit) -> String {
    let unit = match limit.per_seconds {
        1 => "sec",
        60 => "min",
        3600 => "hour",
        _ => "sec",
    };
    format!("{}/{}", limit.max, unit)
}

fn format_field(field: &Field) -> String {
    let mut name = field.name.clone();
    if field.optional {
        name.push('?');
    }
    let mut out = format!("{}: {}", name, format_type(&field.ty));
    if let Some(val) = &field.validation {
        let v = format_validation(val);
        if !v.is_empty() {
            out.push_str(&format!("({})", v));
        }
    }
    out
}

fn format_validation(v: &Validation) -> String {
    let mut parts = Vec::new();
    if let Some(val) = v.min {
        parts.push(format!("min={}", val));
    }
    if let Some(val) = v.max {
        parts.push(format!("max={}", val));
    }
    if let Some(val) = v.min_len {
        parts.push(format!("min_len={}", val));
    }
    if let Some(val) = v.max_len {
        parts.push(format!("max_len={}", val));
    }
    if let Some(val) = v.min_items {
        parts.push(format!("min_items={}", val));
    }
    if let Some(val) = v.max_items {
        parts.push(format!("max_items={}", val));
    }
    if let Some(regex) = &v.regex {
        parts.push(format!("regex=\"{}\"", regex));
    }
    if let Some(format) = &v.format {
        parts.push(format!("format=\"{}\"", format));
    }
    for (key, value) in &v.constraints {
        parts.push(format!("{}=\"{}\"", key, value));
    }
    parts.join(", ")
}

fn format_type(ty: &Type) -> String {
    match ty {
        Type::String => "string".to_string(),
        Type::Int => "int".to_string(),
        Type::Float => "float".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Void => "void".to_string(),
        Type::Any => "any".to_string(),
        Type::Named(name) => name.display(),
        Type::Array(inner) => format!("array<{}>", format_type(inner)),
        Type::Map(inner) => format!("map<{}>", format_type(inner)),
        Type::Union(types) => {
            let inner = types.iter().map(format_type).collect::<Vec<_>>().join(", ");
            format!("union<{}>", inner)
        }
        Type::OneOf(types) => {
            let inner = types.iter().map(format_type).collect::<Vec<_>>().join(", ");
            format!("oneof<{}>", inner)
        }
        Type::Nullable(inner) => format!("nullable<{}>", format_type(inner)),
        Type::Object(fields) => {
            let mut parts = Vec::new();
            for field in fields {
                parts.push(format_field(field));
            }
            format!("{{ {} }}", parts.join(", "))
        }
    }
}

fn format_response(resp: &ResponseSpec) -> String {
    if resp.status == 200 {
        format_type(&resp.ty)
    } else if resp.ty == Type::Void {
        format!("{}", resp.status)
    } else {
        format!("{} {}", resp.status, format_type(&resp.ty))
    }
}

fn format_name_version(name: &str, version: Option<u32>) -> String {
    match version {
        Some(v) => format!("{name}@{v}"),
        None => name.to_string(),
    }
}

fn format_call(call: &CallSpec) -> String {
    let mut target = String::new();
    if let Some(service) = &call.service {
        target.push_str(service);
        target.push('.');
    }
    target.push_str(&call.module);
    target.push('.');
    target.push_str(&call.function);

    let mut out = String::new();
    if call.is_async {
        out.push_str("async ");
    }
    out.push_str(&target);

    if !call.args.is_empty() {
        let mut args = Vec::new();
        for arg in &call.args {
            let value = format_input_ref(&arg.value);
            if let Some(name) = &arg.name {
                args.push(format!("{}={}", name, value));
            } else {
                args.push(value);
            }
        }
        out.push('(');
        out.push_str(&args.join(", "));
        out.push(')');
    }
    out
}

fn format_input_ref(input: &InputRef) -> String {
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

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Head => "HEAD",
        };
        write!(f, "{}", s)
    }
}
