use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use nimesvc::ir::{CallArg, InputRef, InputSource, ResponseSpec, Route, Service, Type, UseScope};

use super::fs as fs_utils;

pub(crate) fn prepare_service(
    service: &Service,
    project: &nimesvc::ir::Project,
    input_dir: &PathBuf,
    out_dir: &PathBuf,
    lang: &nimesvc::ir::Lang,
) -> Result<Service> {
    let mut service_map = std::collections::HashMap::new();
    let mut modules_by_service: std::collections::HashMap<String, Vec<nimesvc::ir::ModuleUse>> =
        std::collections::HashMap::new();
    for svc in &project.services {
        service_map.insert(svc.name.clone(), svc);
        let mut mods = Vec::new();
        mods.extend(project.common.modules.clone());
        mods.extend(svc.common.modules.clone());
        for rpc in &svc.rpc.methods {
            mods.extend(rpc.modules.clone());
        }
        for socket in &svc.sockets.sockets {
            mods.extend(socket.modules.clone());
        }
        modules_by_service.insert(svc.name.clone(), mods);
    }

    let mut modules = Vec::new();
    modules.extend(project.common.modules.clone());
    modules.extend(service.common.modules.clone());
    for rpc in &service.rpc.methods {
        modules.extend(rpc.modules.clone());
    }
    for socket in &service.sockets.sockets {
        modules.extend(socket.modules.clone());
    }

    let mut prepared = service.clone();
    prepared.common.modules = Vec::new();

    let mut used = std::collections::HashSet::new();
    for route in &mut prepared.http.routes {
        if route.healthcheck {
            continue;
        }
        if let Some(svc_name) = route.call.service.clone() {
            let target = service_map.get(&svc_name).ok_or_else(|| {
                anyhow::anyhow!(
                    "Route '{}' in service '{}' references unknown target service '{}'",
                    route.path,
                    service.name,
                    svc_name
                )
            })?;
            let target_modules = modules_by_service.get(&svc_name).ok_or_else(|| {
                anyhow::anyhow!(
                    "Route '{}' in service '{}' references unknown target service '{}'",
                    route.path,
                    service.name,
                    svc_name
                )
            })?;
            let target_mod = target_modules
                .iter()
                .find(|m| m.alias.as_deref().unwrap_or(&m.name) == route.call.module)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Route '{}' in service '{}' calls '{}.{}', but target service '{}' does not declare module '{}'",
                        route.path,
                        service.name,
                        route.call.module,
                        route.call.function,
                        svc_name,
                        route.call.module
                    )
                })?;
            if matches!(target_mod.scope, UseScope::Compile) {
                anyhow::bail!(
                    "Route '{}' in service '{}' calls '{}.{}', but module '{}' in target service '{}' is compile-only. Use `runtime` or the default scope.",
                    route.path,
                    service.name,
                    route.call.module,
                    route.call.function,
                    route.call.module,
                    svc_name
                );
            }
            route.call.service_base = Some(service_base_url(target));
        } else {
            used.insert(route.call.module.clone());
        }
    }
    for rpc in &mut prepared.rpc.methods {
        if let Some(svc_name) = rpc.call.service.clone() {
            if svc_name != prepared.name {
                anyhow::bail!(
                    "RPC '{}' in service '{}' calls service '{}', but RPC handlers may only call local modules",
                    rpc.name,
                    prepared.name,
                    svc_name
                );
            }
        }
        used.insert(rpc.call.module.clone());
    }
    for socket in &mut prepared.sockets.sockets {
        for msg in socket.inbound.iter().chain(socket.outbound.iter()) {
            used.insert(msg.handler.module.clone());
        }
    }

    let mut by_name = std::collections::HashMap::new();
    for m in &modules {
        let key = m.alias.as_deref().unwrap_or(&m.name).to_string();
        if let Some(existing) = by_name.get(&key) {
            if existing != m {
                anyhow::bail!(
                    "Duplicate module alias '{}'. Rename one of the `use` declarations or add an explicit alias.",
                    key
                );
            }
            continue;
        }
        by_name.insert(key, m.clone());
    }

    for name in &used {
        if !by_name.contains_key(name) {
            anyhow::bail!(
                "Module '{}' is used by service '{}' but is not declared via `use`",
                name,
                service.name
            );
        }
        let module = by_name.get(name).unwrap();
        if matches!(module.scope, UseScope::Compile) {
            anyhow::bail!(
                "Module '{}' in service '{}' is compile-only but used at runtime. Use `runtime` or the default scope.",
                name,
                service.name
            );
        }
    }

    let mut uniq = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in modules {
        let key = m.alias.as_deref().unwrap_or(&m.name);
        if seen.insert(key.to_string()) {
            uniq.push(m);
        }
    }
    let modules = uniq;

    let modules_base = match lang {
        nimesvc::ir::Lang::Rust | nimesvc::ir::Lang::TypeScript => out_dir.join("src"),
        nimesvc::ir::Lang::Go => out_dir.join("modules"),
    };
    fs::create_dir_all(&modules_base)
        .with_context(|| format!("Failed to create '{}'", modules_base.display()))?;

    for mut module in modules {
        if let Some(path) = &module.path {
            let raw_path = std::path::Path::new(path);
            let src_path = if raw_path.is_absolute() {
                raw_path.to_path_buf()
            } else {
                input_dir.join(raw_path)
            };
            let ext = src_path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !is_module_compatible(lang, ext) {
                anyhow::bail!(
                    "Module '{}' is not compatible with {:?}. Expected {} module files.",
                    path,
                    lang,
                    expected_module_extension(lang)
                );
            }
            if matches!(lang, nimesvc::ir::Lang::Go) {
                let pkg = module.alias.as_deref().unwrap_or(&module.name);
                let filename = src_path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid module path '{}'", path))?;
                let dest_dir = modules_base.join(pkg);
                fs::create_dir_all(&dest_dir)
                    .with_context(|| format!("Failed to create '{}'", dest_dir.display()))?;
                let dest_path = dest_dir.join(filename);
                let src_content = fs::read_to_string(&src_path)
                    .with_context(|| format!("Failed to read module '{}'", src_path.display()))?;
                let rewritten = rewrite_go_package(&src_content, pkg);
                fs::write(&dest_path, rewritten)
                    .with_context(|| format!("Failed to write module '{}'", dest_path.display()))?;
                module.path = Some(pkg.to_string());
            } else {
                let rel_path = fs_utils::normalize_module_rel_path(path)?;
                let dest_path = modules_base.join(&rel_path);
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("Failed to create '{}'", parent.display()))?;
                }
                fs::copy(&src_path, &dest_path)
                    .with_context(|| format!("Failed to copy module '{}'", src_path.display()))?;
                module.path = Some(fs_utils::path_to_import_string(&rel_path));
            }
        }
        prepared.common.modules.push(module);
    }

    prepared.common.auth = service.common.auth.clone().or(project.common.auth.clone());
    let mut mw = project.common.middleware.clone();
    mw.extend(service.common.middleware.clone());
    prepared.common.middleware = mw;
    prepared.common.env = service.common.env.clone();
    prepared.common.rate_limit = service.common.rate_limit.clone();

    let incoming = collect_incoming_calls(service, project)?;
    if !incoming.is_empty() {
        let mut existing_paths = std::collections::HashSet::new();
        for route in &prepared.http.routes {
            existing_paths.insert(route.path.clone());
        }
        for route in incoming {
            if existing_paths.contains(&route.path) {
                anyhow::bail!(
                    "Generated internal route '{}' for service '{}' conflicts with an existing route",
                    route.path,
                    prepared.name
                );
            }
            prepared.http.routes.push(route);
        }
    }

    Ok(prepared)
}

fn rewrite_go_package(src: &str, package: &str) -> String {
    let mut out = String::new();
    let mut replaced = false;
    for line in src.lines() {
        if !replaced && line.trim_start().starts_with("package ") {
            out.push_str(&format!("package {}\n", package));
            replaced = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if replaced {
        out
    } else {
        format!("package {}\n\n{}", package, src)
    }
}

fn is_module_compatible(lang: &nimesvc::ir::Lang, ext: &str) -> bool {
    match lang {
        nimesvc::ir::Lang::Rust => ext == "rs",
        nimesvc::ir::Lang::TypeScript => ext == "ts" || ext == "js",
        nimesvc::ir::Lang::Go => ext == "go",
    }
}

fn expected_module_extension(lang: &nimesvc::ir::Lang) -> &'static str {
    match lang {
        nimesvc::ir::Lang::Rust => "`.rs`",
        nimesvc::ir::Lang::TypeScript => "`.ts` or `.js`",
        nimesvc::ir::Lang::Go => "`.go`",
    }
}

fn service_base_url(service: &Service) -> String {
    if let Some(base) = &service.common.base_url {
        return base.clone();
    }
    let address = service.common.address.as_deref().unwrap_or("127.0.0.1");
    let address = if address == "0.0.0.0" {
        "127.0.0.1"
    } else {
        address
    };
    let port = service.common.port.unwrap_or(3000);
    format!("http://{}:{}", address, port)
}

struct IncomingSpec {
    fields: Vec<nimesvc::ir::Field>,
    response: nimesvc::ir::Type,
    is_async: bool,
}

fn collect_incoming_calls(
    target_service: &Service,
    project: &nimesvc::ir::Project,
) -> Result<Vec<Route>> {
    let mut target_modules = Vec::new();
    target_modules.extend(project.common.modules.clone());
    target_modules.extend(target_service.common.modules.clone());

    let mut module_by_name = std::collections::HashMap::new();
    for module in &target_modules {
        let key = module.alias.as_deref().unwrap_or(&module.name).to_string();
        module_by_name.insert(key, module.clone());
    }

    let mut incoming: std::collections::HashMap<(String, String), IncomingSpec> =
        std::collections::HashMap::new();

    for caller in &project.services {
        for route in &caller.http.routes {
            if route.call.service.as_deref() != Some(&target_service.name) {
                continue;
            }
            if !module_by_name.contains_key(&route.call.module) {
                anyhow::bail!(
                    "Service '{}' route '{}' calls '{}.{}', but target service '{}' does not declare module '{}'",
                    caller.name,
                    route.path,
                    route.call.module,
                    route.call.function,
                    target_service.name,
                    route.call.module
                );
            }
            let module = module_by_name.get(&route.call.module).ok_or_else(|| {
                anyhow::anyhow!(
                    "Service '{}' route '{}' calls '{}.{}', but target service '{}' does not declare module '{}'",
                    caller.name,
                    route.path,
                    route.call.module,
                    route.call.function,
                    target_service.name,
                    route.call.module
                )
            })?;
            if matches!(module.scope, UseScope::Compile) {
                anyhow::bail!(
                    "Service '{}' route '{}' calls '{}.{}', but module '{}' in target service '{}' is compile-only",
                    caller.name,
                    route.path,
                    route.call.module,
                    route.call.function,
                    route.call.module,
                    target_service.name
                );
            }

            let resp = primary_response(route);
            let call_key = (route.call.module.clone(), route.call.function.clone());
            let arg_fields = route
                .call
                .args
                .iter()
                .enumerate()
                .map(|(idx, arg)| {
                    let resolved = resolve_input_ref_type(caller, route, &arg.value)?;
                    Ok(nimesvc::ir::Field {
                        name: call_arg_name(arg, idx),
                        ty: resolved.ty,
                        optional: resolved.optional,
                        validation: resolved.validation,
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            let is_async = infer_call_async(route);
            let spec = IncomingSpec {
                fields: arg_fields,
                response: resp.ty.clone(),
                is_async,
            };
            incoming
                .entry(call_key)
                .and_modify(|existing| {
                    if existing.fields != spec.fields || existing.response != spec.response {
                        existing.fields = spec.fields.clone();
                        existing.response = spec.response.clone();
                        existing.is_async = existing.is_async || spec.is_async;
                    }
                })
                .or_insert(spec);
        }
    }

    let mut routes = Vec::new();
    for ((module, func), spec) in incoming {
        let route = Route {
            method: nimesvc::ir::HttpMethod::Post,
            path: format!("/__call/{}/{}", module, func),
            input: nimesvc::ir::Input {
                path: vec![],
                query: vec![],
                body: spec.fields.clone(),
            },
            responses: vec![ResponseSpec {
                status: 200,
                ty: spec.response,
            }],
            auth: None,
            middleware: vec![],
            call: nimesvc::ir::CallSpec {
                service: None,
                service_base: None,
                module: module.clone(),
                function: func.clone(),
                args: spec
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| CallArg {
                        name: Some(call_arg_name_from_field(field, idx)),
                        value: InputRef {
                            source: InputSource::Body,
                            path: vec![field.name.clone()],
                        },
                    })
                    .collect(),
                is_async: spec.is_async,
            },
            rate_limit: None,
            healthcheck: false,
            headers: vec![],
            internal: true,
        };
        routes.push(route);
    }

    Ok(routes)
}

fn primary_response(route: &Route) -> &ResponseSpec {
    route
        .responses
        .iter()
        .find(|r| (200..300).contains(&r.status))
        .unwrap_or_else(|| &route.responses[0])
}

fn infer_call_async(route: &Route) -> bool {
    route.call.is_async
}

fn call_arg_name(arg: &CallArg, idx: usize) -> String {
    arg.name
        .as_ref()
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("arg{}", idx))
}

fn call_arg_name_from_field(field: &nimesvc::ir::Field, idx: usize) -> String {
    if field.name.is_empty() {
        format!("arg{}", idx)
    } else {
        field.name.clone()
    }
}

struct ResolvedType {
    ty: Type,
    optional: bool,
    validation: Option<nimesvc::ir::Validation>,
}

fn resolve_input_ref_type(
    service: &Service,
    route: &Route,
    input: &InputRef,
) -> Result<ResolvedType> {
    let fields: &[nimesvc::ir::Field] = match input.source {
        InputSource::Path => &route.input.path,
        InputSource::Query => &route.input.query,
        InputSource::Body => &route.input.body,
        InputSource::Headers => &route.headers,
        InputSource::Input => &[],
    };
    if input.path.is_empty() {
        return Ok(ResolvedType {
            ty: Type::Object(fields.to_vec()),
            optional: false,
            validation: None,
        });
    }
    let current = fields
        .iter()
        .find(|f| f.name == input.path[0])
        .cloned()
        .unwrap_or(nimesvc::ir::Field {
            name: input.path[0].clone(),
            ty: Type::Any,
            optional: false,
            validation: None,
        });
    let mut optional = current.optional;
    let mut ty = current.ty.clone();
    let mut validation = current.validation.clone();
    for segment in input.path.iter().skip(1) {
        let resolved = resolve_field_in_type(service, &ty, segment)?;
        optional = optional || resolved.optional;
        ty = resolved.ty;
        validation = resolved.validation;
    }
    Ok(ResolvedType {
        ty,
        optional,
        validation,
    })
}

fn resolve_field_in_type(service: &Service, ty: &Type, name: &str) -> Result<ResolvedType> {
    let fields = match ty {
        Type::Object(fields) => fields.clone(),
        Type::Named(name) => {
            let def = service
                .schema
                .types
                .iter()
                .find(|t| t.name == *name)
                .ok_or_else(|| anyhow::anyhow!("Unknown type '{}'", name.display()))?;
            def.fields.clone()
        }
        _ => {
            return Ok(ResolvedType {
                ty: Type::Any,
                optional: false,
                validation: None,
            });
        }
    };
    let field = fields
        .iter()
        .find(|f| f.name == name)
        .cloned()
        .unwrap_or(nimesvc::ir::Field {
            name: name.to_string(),
            ty: Type::Any,
            optional: false,
            validation: None,
        });
    Ok(ResolvedType {
        ty: field.ty.clone(),
        optional: field.optional,
        validation: field.validation.clone(),
    })
}

impl std::fmt::Debug for ResolvedType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedType")
            .field("ty", &self.ty)
            .field("optional", &self.optional)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use nimesvc::ir::Lang;
    use nimesvc::parser::parse_project;

    use super::prepare_service;

    #[test]
    fn prepare_service_allows_healthcheck_without_use_module() {
        let src = r#"
service API:
    GET "/health":
        response 200
        healthcheck
"#;
        let project = parse_project(src).unwrap();
        let service = project.services.first().unwrap();
        let input_dir = PathBuf::from(".");
        let mut out_dir = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        out_dir.push(format!("nimesvc-prep-test-{stamp}"));
        fs::create_dir_all(&out_dir).unwrap();

        let prepared =
            prepare_service(service, &project, &input_dir, &out_dir, &Lang::Rust).unwrap();
        assert_eq!(prepared.http.routes.len(), 1);
        assert!(prepared.http.routes[0].healthcheck);

        let _ = fs::remove_dir_all(&out_dir);
    }
}
