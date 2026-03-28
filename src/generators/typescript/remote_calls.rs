use crate::ir::{InputRef, InputSource, Route, Service, Type};

use super::util::{primary_response, remote_call_fn_name, remote_call_type_name, ts_client_type};

pub(super) fn build_remote_calls_ts(service: &Service) -> String {
    let calls = collect_remote_calls(service);
    let mut out = String::new();
    out.push_str("import { Request } from 'express';\n");
    out.push_str("import * as Types from './types';\n\n");
    out.push_str(
        r#"function buildForwardHeaders(req: Request): Record<string, string> {
  const headers: Record<string, string> = {};
  const auth = req.headers["authorization"];
  if (typeof auth === "string") {
    headers["authorization"] = auth;
  }
  for (const [key, value] of Object.entries(req.headers)) {
    if (!key.startsWith("x-")) continue;
    if (typeof value === "string") {
      headers[key] = value;
    } else if (Array.isArray(value) && value.length > 0) {
      headers[key] = value[0];
    }
  }
  return headers;
}

async function callRemote<T>(base: string, path: string, payload: Record<string, unknown>, req: Request): Promise<T> {
  const headers = buildForwardHeaders(req);
  headers["content-type"] = "application/json";
  const resp = await fetch(`${base}${path}`, {
    method: "POST",
    headers,
    body: JSON.stringify(payload),
  });
  if (!resp.ok) {
    throw new Error(`Remote call failed: ${resp.status}`);
  }
  const text = await resp.text();
  if (!text) {
    return undefined as T;
  }
  try {
    return JSON.parse(text) as T;
  } catch {
    return text as unknown as T;
  }
}

"#,
    );

    for call in calls {
        let type_name = remote_call_type_name(&call.name);
        let type_fields = call
            .args
            .iter()
            .map(|arg| {
                let ty = ts_client_type(&arg.ty);
                if arg.optional {
                    format!("  {}?: {};", arg.name, ty)
                } else {
                    format!("  {}: {};", arg.name, ty)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let resp_ty = ts_client_type(&call.response);
        out.push_str(&format!(
            "export type {name}Request = {{\n{fields}\n}};\n",
            name = type_name,
            fields = type_fields
        ));
        out.push_str(&format!(
            "export type {name}Response = {resp};\n\n",
            name = type_name,
            resp = resp_ty
        ));

        let params = call
            .args
            .iter()
            .map(|arg| {
                let ty = ts_client_type(&arg.ty);
                if arg.optional {
                    format!("{}?: {}", arg.name, ty)
                } else {
                    format!("{}: {}", arg.name, ty)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let payload = call
            .args
            .iter()
            .map(|arg| format!("{}: {}", arg.name, arg.name))
            .collect::<Vec<_>>()
            .join(", ");
        let resp_ty = ts_client_type(&call.response);
        out.push_str(&format!(
            "export async function {name}({params}{comma}req: Request): Promise<{resp_ty}> {{\n  return callRemote<{resp_ty}>(\"{base}\", \"{path}\", {{{payload}}}, req);\n}}\n\n",
            name = call.name,
            params = params,
            comma = if params.is_empty() { "" } else { ", " },
            resp_ty = resp_ty,
            base = call.base,
            path = call.path,
            payload = payload
        ));
    }
    out
}

struct RemoteCallSpec {
    name: String,
    base: String,
    path: String,
    args: Vec<RemoteArgSpec>,
    response: Type,
}

struct RemoteArgSpec {
    name: String,
    ty: Type,
    optional: bool,
}

fn collect_remote_calls(service: &Service) -> Vec<RemoteCallSpec> {
    let mut by_key: std::collections::BTreeMap<String, RemoteCallSpec> =
        std::collections::BTreeMap::new();
    for route in &service.http.routes {
        let Some(base) = &route.call.service_base else {
            continue;
        };
        let key = format!(
            "{}::{}::{}::{}",
            route.call.service.as_deref().unwrap_or("service"),
            route.call.module,
            route.call.function,
            base
        );
        if by_key.contains_key(&key) {
            continue;
        }
        let mut args = Vec::new();
        for (idx, arg) in route.call.args.iter().enumerate() {
            let name = arg.name.clone().unwrap_or_else(|| format!("arg{}", idx));
            let resolved = resolve_input_ref_type(service, route, &arg.value);
            args.push(RemoteArgSpec {
                name,
                ty: resolved.ty,
                optional: resolved.optional,
            });
        }
        let resp = primary_response(route);
        let path = format!("/__call/{}/{}", route.call.module, route.call.function);
        let spec = RemoteCallSpec {
            name: remote_call_fn_name(route),
            base: base.clone(),
            path,
            args,
            response: resp.ty.clone(),
        };
        by_key.insert(key, spec);
    }
    by_key.into_values().collect()
}

struct ResolvedType {
    ty: Type,
    optional: bool,
}

fn resolve_input_ref_type(service: &Service, route: &Route, input: &InputRef) -> ResolvedType {
    let fields: &[crate::ir::Field] = match input.source {
        InputSource::Path => &route.input.path,
        InputSource::Query => &route.input.query,
        InputSource::Body => &route.input.body,
        InputSource::Headers => &route.headers,
        InputSource::Input => &[],
    };
    if input.path.is_empty() {
        return ResolvedType {
            ty: Type::Object(fields.to_vec()),
            optional: false,
        };
    }
    let current = fields
        .iter()
        .find(|f| f.name == input.path[0])
        .cloned()
        .unwrap_or(crate::ir::Field {
            name: input.path[0].clone(),
            ty: Type::Any,
            optional: false,
            validation: None,
        });
    let mut optional = current.optional;
    let mut ty = current.ty.clone();
    for segment in input.path.iter().skip(1) {
        let resolved = resolve_field_in_type(service, &ty, segment);
        optional = optional || resolved.optional;
        ty = resolved.ty;
    }
    ResolvedType { ty, optional }
}

fn resolve_field_in_type(service: &Service, ty: &Type, name: &str) -> ResolvedType {
    let fields = match ty {
        Type::Object(fields) => fields.clone(),
        Type::Named(name) => service
            .schema
            .types
            .iter()
            .find(|t| t.name == *name)
            .map(|t| t.fields.clone())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let field = fields
        .iter()
        .find(|f| f.name == name)
        .cloned()
        .unwrap_or(crate::ir::Field {
            name: name.to_string(),
            ty: Type::Any,
            optional: false,
            validation: None,
        });
    ResolvedType {
        ty: field.ty,
        optional: field.optional,
    }
}
