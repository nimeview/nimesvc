use crate::ir::{Service, Type};

use super::util::{
    go_field_name, go_type, primary_response, remote_call_fn_name, remote_call_type_name,
    resolve_input_ref_type, type_uses_types_pkg,
};

pub(super) fn build_remote_calls_go(service: &Service, module_name: &str) -> String {
    let calls = collect_remote_calls(service);
    if calls.is_empty() {
        return String::new();
    }
    let needs_io = calls.iter().any(|c| c.response == Type::String);
    let needs_types = calls.iter().any(|c| {
        type_uses_types_pkg(&c.response) || c.args.iter().any(|a| type_uses_types_pkg(&a.ty))
    });
    let mut out = String::new();
    out.push_str("package main\n\n");
    out.push_str("import (\n");
    out.push_str("    \"bytes\"\n");
    out.push_str("    \"encoding/json\"\n");
    out.push_str("    \"fmt\"\n");
    out.push_str("    \"net/http\"\n");
    out.push_str("    \"strings\"\n");
    if needs_io {
        out.push_str("    \"io\"\n");
    }
    if needs_types {
        out.push_str(&format!("    \"{}/types\"\n", module_name));
    }
    out.push_str(")\n\n");

    for call in calls {
        let type_name = remote_call_type_name(&call.name);
        out.push_str(&format!("type {}Request struct {{\n", type_name));
        for arg in &call.args {
            let go_name = go_field_name(&arg.name);
            let go_ty = go_type(&arg.ty, true);
            if arg.optional {
                out.push_str(&format!(
                    "    {} *{} `json:\"{},omitempty\"`\n",
                    go_name, go_ty, arg.name
                ));
            } else {
                out.push_str(&format!(
                    "    {} {} `json:\"{}\"`\n",
                    go_name, go_ty, arg.name
                ));
            }
        }
        out.push_str("}\n");
        out.push_str(&format!(
            "type {}Response = {}\n\n",
            type_name,
            go_type(&call.response, true)
        ));

        let params = call
            .args
            .iter()
            .map(|arg| {
                let go_ty = go_type(&arg.ty, true);
                if arg.optional {
                    format!("{} *{}", arg.name, go_ty)
                } else {
                    format!("{} {}", arg.name, go_ty)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let payload_fields = call
            .args
            .iter()
            .map(|arg| format!("\"{}\": {}", arg.name, arg.name))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "func {name}({params}{comma}req *http.Request) ({ret}, error) {{\n",
            name = call.name,
            params = params,
            comma = if params.is_empty() { "" } else { ", " },
            ret = go_type(&call.response, true)
        ));
        out.push_str(&format!(
            "    payload := map[string]any{{{}}}\n",
            payload_fields
        ));
        out.push_str("    data, _ := json.Marshal(payload)\n");
        out.push_str(&format!(
            "    httpReq, err := http.NewRequest(\"POST\", \"{}{}\", bytes.NewReader(data))\n",
            call.base, call.path
        ));
        out.push_str("    if err != nil { return ");
        out.push_str(&zero_value(&call.response));
        out.push_str(", err }\n");
        out.push_str("    httpReq.Header.Set(\"Content-Type\", \"application/json\")\n");
        out.push_str("    if req != nil {\n");
        out.push_str("        for k, v := range req.Header {\n");
        out.push_str("            key := strings.ToLower(k)\n");
        out.push_str(
            "            if key == \"authorization\" || strings.HasPrefix(key, \"x-\") {\n",
        );
        out.push_str("                for _, item := range v {\n");
        out.push_str("                    httpReq.Header.Add(k, item)\n");
        out.push_str("                }\n");
        out.push_str("            }\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("    resp, err := http.DefaultClient.Do(httpReq)\n");
        out.push_str("    if err != nil { return ");
        out.push_str(&zero_value(&call.response));
        out.push_str(", err }\n");
        out.push_str("    defer resp.Body.Close()\n");
        out.push_str("    if resp.StatusCode < 200 || resp.StatusCode >= 300 {\n");
        out.push_str("        return ");
        out.push_str(&zero_value(&call.response));
        out.push_str(", fmt.Errorf(\"remote call failed: %d\", resp.StatusCode)\n");
        out.push_str("    }\n");
        if call.response == Type::Void {
            out.push_str("    return ");
            out.push_str(&zero_value(&call.response));
            out.push_str(", nil\n}\n\n");
        } else if call.response == Type::String {
            out.push_str("    body, _ := io.ReadAll(resp.Body)\n");
            out.push_str("    var value string\n");
            out.push_str("    if err := json.Unmarshal(body, &value); err == nil {\n");
            out.push_str("        return value, nil\n    }\n");
            out.push_str("    return string(body), nil\n}\n\n");
        } else {
            out.push_str(&format!(
                "    var value {}\n",
                go_type(&call.response, true)
            ));
            out.push_str("    if err := json.NewDecoder(resp.Body).Decode(&value); err != nil {\n");
            out.push_str("        return ");
            out.push_str(&zero_value(&call.response));
            out.push_str(", err\n    }\n");
            out.push_str("    return value, nil\n}\n\n");
        }
    }

    out
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
        let spec = RemoteCallSpec {
            name: remote_call_fn_name(route),
            base: base.clone(),
            path: format!("/__call/{}/{}", route.call.module, route.call.function),
            args,
            response: resp.ty.clone(),
        };
        by_key.insert(key, spec);
    }
    by_key.into_values().collect()
}

fn zero_value(ty: &Type) -> String {
    match ty {
        Type::String => "\"\"".to_string(),
        Type::Int | Type::Float => "0".to_string(),
        Type::Bool => "false".to_string(),
        Type::Array(_) => "nil".to_string(),
        Type::Map(_) => "nil".to_string(),
        Type::Union(_) | Type::OneOf(_) => "nil".to_string(),
        Type::Nullable(_) => "nil".to_string(),
        Type::Object(_) => "nil".to_string(),
        Type::Any => "nil".to_string(),
        Type::Void => "struct{}{}".to_string(),
        Type::Named(name) => format!("types.{}{{}}", name.code_name()),
    }
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
