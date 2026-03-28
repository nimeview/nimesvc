use crate::ir::Service;

pub(super) fn build_package_json(service: &Service) -> String {
    let mut deps = vec!["express\": \"^4.18.2\"".to_string()];
    if !service.sockets.sockets.is_empty() {
        deps.push("ws\": \"^8.17.0\"".to_string());
    }
    if service
        .events
        .config
        .as_ref()
        .map(|c| matches!(c.broker, crate::ir::EventsBroker::Redis))
        .unwrap_or(false)
    {
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
  "name": "{name}-api",
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
    "@types/express": "^4.17.21",
    "@types/ws": "^8.5.12",
    "typescript": "^5.5.0",
    "ts-node": "^10.9.2"
  }}
}}
"#,
        name = service.name.to_lowercase(),
        deps_block = deps_block
    )
}
