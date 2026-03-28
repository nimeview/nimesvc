use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::ir::{Service, Type};

use super::util::{
    GO_FN_PREFIX, go_type, go_wrapper_name, resolve_input_ref_type, to_go_func_name,
};

pub(super) fn write_go_module_wrappers(service: &Service, out_dir: &Path) -> Result<()> {
    let mut by_module: HashMap<String, HashMap<String, (Vec<(String, Type, bool)>, Type)>> =
        HashMap::new();

    for route in &service.http.routes {
        if route.call.service_base.is_some() {
            continue;
        }
        if route.call.module.is_empty() {
            continue;
        }
        let module = route.call.module.clone();
        let module_decl = service
            .common
            .modules
            .iter()
            .find(|m| m.alias.as_deref().unwrap_or(&m.name) == module);
        let Some(module_decl) = module_decl else {
            continue;
        };
        if module_decl.path.is_none() {
            continue;
        }
        let func = route.call.function.clone();
        if func.starts_with(GO_FN_PREFIX) {
            continue;
        }
        let entry = by_module.entry(module).or_insert_with(HashMap::new);
        if entry.contains_key(&func) {
            continue;
        }
        let resp = super::util::primary_response(route);
        let mut args = Vec::new();
        for (idx, arg) in route.call.args.iter().enumerate() {
            let resolved = resolve_input_ref_type(service, route, &arg.value);
            args.push((format!("arg{}", idx + 1), resolved.ty, resolved.optional));
        }
        entry.insert(func, (args, resp.ty.clone()));
    }

    for (module, funcs) in by_module {
        let module_dir = out_dir.join("modules").join(&module);
        fs::create_dir_all(&module_dir)
            .with_context(|| format!("Failed to create '{}'", module_dir.display()))?;

        let mut out = String::new();
        out.push_str(&format!("package {}\n\n", module));

        for (raw, (args, ret)) in funcs {
            let wrapper = go_wrapper_name(&raw);
            let mut params = Vec::new();
            for (name, ty, optional) in args {
                let mut t = go_type(&ty, false);
                if optional {
                    t = format!("*{}", t);
                }
                params.push(format!("{} {}", name, t));
            }
            let sig = params.join(", ");
            let call_args = params
                .iter()
                .map(|p| p.split_whitespace().next().unwrap_or(""))
                .collect::<Vec<_>>()
                .join(", ");

            let go_target = to_go_func_name(&raw);
            if ret == Type::Void {
                out.push_str(&format!(
                    "func {wrapper}({sig}) {{\n    {target}({call_args})\n}}\n\n",
                    wrapper = wrapper,
                    sig = sig,
                    target = go_target,
                    call_args = call_args
                ));
            } else {
                let ret_ty = go_type(&ret, false);
                out.push_str(&format!(
                    "func {wrapper}({sig}) {ret_ty} {{\n    return {target}({call_args})\n}}\n\n",
                    wrapper = wrapper,
                    sig = sig,
                    ret_ty = ret_ty,
                    target = go_target,
                    call_args = call_args
                ));
            }
        }

        let wrapper_path = module_dir.join("nimesvc_wrap.go");
        fs::write(&wrapper_path, out)
            .with_context(|| format!("Failed to write '{}'", wrapper_path.display()))?;
    }

    Ok(())
}
