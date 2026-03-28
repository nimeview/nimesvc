use std::collections::BTreeMap;

use crate::ir::{EnumDef, Field, RpcDef, Service, Type};

pub fn build_proto(service: &Service) -> String {
    if service.rpc.methods.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("syntax = \"proto3\";\n\n");
    out.push_str(&format!(
        "package nimesvc.{};\n",
        service.name.to_lowercase()
    ));
    out.push_str(&format!(
        "option go_package = \"nimesvc/{}-grpc/proto;rpc\";\n\n",
        service.name.to_lowercase()
    ));

    let mut needs_empty = false;
    let mut needs_struct = false;

    let mut messages: BTreeMap<String, Vec<Field>> = BTreeMap::new();
    let mut enums: Vec<EnumDef> = Vec::new();

    for en in &service.schema.enums {
        enums.push(en.clone());
    }
    for ty in &service.schema.types {
        messages.insert(ty.name.code_name(), ty.fields.clone());
    }

    for rpc in &service.rpc.methods {
        let req_name = format!("{}Request", rpc.code_name());
        if !messages.contains_key(&req_name) {
            messages.insert(req_name, rpc.input.clone());
        }
        match &rpc.output {
            Type::Void => {
                needs_empty = true;
            }
            Type::Object(fields) => {
                let resp_name = format!("{}Response", rpc.code_name());
                messages.insert(resp_name, fields.clone());
            }
            Type::Any => {
                needs_struct = true;
            }
            Type::Array(_) | Type::String | Type::Int | Type::Float | Type::Bool => {
                let resp_name = format!("{}Response", rpc.code_name());
                if !messages.contains_key(&resp_name) {
                    let field = Field {
                        name: "value".to_string(),
                        ty: rpc.output.clone(),
                        optional: false,
                        validation: None,
                    };
                    messages.insert(resp_name, vec![field]);
                }
            }
            _ => {}
        }
        if uses_struct(&rpc.output) {
            needs_struct = true;
        }
    }

    if needs_empty {
        out.push_str("import \"google/protobuf/empty.proto\";\n");
    }
    if needs_struct {
        out.push_str("import \"google/protobuf/struct.proto\";\n");
    }
    if needs_empty || needs_struct {
        out.push('\n');
    }

    for en in &enums {
        let enum_name = en.name.code_name();
        out.push_str(&format!("enum {} {{\n", enum_name));
        for (idx, var) in en.variants.iter().enumerate() {
            let val = var.value.unwrap_or(idx as i64);
            out.push_str(&format!("  {} = {};\n", var.name, val));
        }
        out.push_str("}\n\n");
    }

    for (name, fields) in messages {
        out.push_str(&format!("message {} {{\n", name));
        for (idx, field) in fields.iter().enumerate() {
            let (ty, repeated) = proto_type(&field.ty);
            let number = idx + 1;
            let mut line = String::new();
            if repeated {
                line.push_str("  repeated ");
            } else if field.optional && is_optional_scalar(&field.ty) {
                line.push_str("  optional ");
            } else {
                line.push_str("  ");
            }
            line.push_str(&format!("{} {} = {};", ty, field.name, number));
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("}\n\n");
    }

    out.push_str(&format!("service {} {{\n", service.name));
    for rpc in &service.rpc.methods {
        let req = format!("{}Request", rpc.code_name());
        let resp = match &rpc.output {
            Type::Void => "google.protobuf.Empty".to_string(),
            Type::Object(_) => format!("{}Response", rpc.code_name()),
            Type::Any | Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) => {
                "google.protobuf.Struct".to_string()
            }
            Type::Named(name) => name.code_name(),
            _ => format!("{}Response", rpc.code_name()),
        };
        out.push_str(&format!(
            "  rpc {} ({}) returns ({});\n",
            rpc.code_name(),
            req,
            resp
        ));
    }
    out.push_str("}\n");

    out
}

fn proto_type(ty: &Type) -> (String, bool) {
    match ty {
        Type::String => ("string".to_string(), false),
        Type::Int => ("int64".to_string(), false),
        Type::Float => ("double".to_string(), false),
        Type::Bool => ("bool".to_string(), false),
        Type::Array(inner) => {
            let (inner_ty, _) = proto_type(inner);
            (inner_ty, true)
        }
        Type::Object(_) => ("bytes".to_string(), false),
        Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) => {
            ("google.protobuf.Struct".to_string(), false)
        }
        Type::Any => ("google.protobuf.Struct".to_string(), false),
        Type::Void => ("google.protobuf.Empty".to_string(), false),
        Type::Named(name) => (name.code_name(), false),
    }
}

fn uses_struct(ty: &Type) -> bool {
    match ty {
        Type::Any | Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) => true,
        Type::Array(inner) => uses_struct(inner),
        Type::Object(fields) => fields.iter().any(|f| uses_struct(&f.ty)),
        _ => false,
    }
}

fn is_optional_scalar(ty: &Type) -> bool {
    matches!(ty, Type::String | Type::Int | Type::Float | Type::Bool)
}

impl RpcDef {
    pub fn code_name(&self) -> String {
        let base = match self.version {
            Some(v) => format!("{}V{}", self.name, v),
            None => self.name.clone(),
        };
        base
    }
}
