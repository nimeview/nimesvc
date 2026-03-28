use crate::ir::Service;

use super::util::{rust_type, rust_type_name};

pub(crate) fn build_types_rs(service: &Service) -> String {
    if service.schema.types.is_empty()
        && service.schema.enums.is_empty()
        && service.events.definitions.is_empty()
        && service.sockets.sockets.is_empty()
    {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("#![allow(dead_code)]\n");
    out.push_str("use serde::{Serialize, Deserialize};\n\n");
    out.push_str(&render_type_defs(service));
    if !service.sockets.sockets.is_empty() {
        out.push_str(&render_socket_payloads());
    }
    out
}

fn render_type_defs(service: &Service) -> String {
    let mut out = String::new();
    for en in &service.schema.enums {
        out.push_str("#[derive(Serialize, Deserialize)]\n");
        let is_numeric = en.variants.iter().all(|v| v.value.is_some());
        if is_numeric {
            out.push_str("#[repr(i64)]\n");
        }
        out.push_str(&format!("pub enum {} {{\n", rust_type_name(&en.name)));
        for var in &en.variants {
            if let Some(val) = var.value {
                out.push_str(&format!("    {} = {},\n", var.name, val));
            } else {
                out.push_str(&format!("    {},\n", var.name));
            }
        }
        out.push_str("}\n\n");
    }
    for ty in &service.schema.types {
        out.push_str("#[derive(Serialize, Deserialize)]\n");
        out.push_str(&format!("pub struct {} {{\n", rust_type_name(&ty.name)));
        for field in &ty.fields {
            let ty_name = rust_type(&field.ty);
            if field.optional {
                out.push_str(&format!("    pub {}: Option<{}>,\n", field.name, ty_name));
            } else {
                out.push_str(&format!("    pub {}: {},\n", field.name, ty_name));
            }
        }
        out.push_str("}\n\n");
    }
    for ev in &service.events.definitions {
        let alias = format!("{}Event", ev.name.code_name());
        let payload_ty = rust_type(&ev.payload);
        out.push_str(&format!("pub type {} = {};\n\n", alias, payload_ty));
    }
    out
}

fn render_socket_payloads() -> String {
    let mut out = String::new();
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct JoinPayload {\n    pub user_id: Option<String>,\n    pub meta: Option<serde_json::Value>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct ExitPayload {\n    pub reason: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct MessageInPayload {\n    pub id: Option<String>,\n    pub text: Option<String>,\n    pub data: Option<serde_json::Value>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct MessageOutPayload {\n    pub id: Option<String>,\n    pub text: Option<String>,\n    pub data: Option<serde_json::Value>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct TypingPayload {\n    pub user_id: Option<String>,\n    pub typing: Option<bool>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct PingPayload {\n    pub ts: Option<i64>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct PongPayload {\n    pub ts: Option<i64>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct AuthPayload {\n    pub token: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct SubscribePayload {\n    pub topic: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct UnsubscribePayload {\n    pub topic: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct RoomJoinPayload {\n    pub room: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct RoomLeavePayload {\n    pub room: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct AckPayload {\n    pub id: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct ReceiptPayload {\n    pub id: Option<String>,\n    pub status: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct UserJoinedPayload {\n    pub user_id: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct UserLeftPayload {\n    pub user_id: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct ErrorPayload {\n    pub message: Option<String>,\n    pub code: Option<String>,\n}\n\n");
    out.push_str("#[derive(Serialize, Deserialize)]\n");
    out.push_str("pub struct ServerNoticePayload {\n    pub message: Option<String>,\n    pub level: Option<String>,\n}\n\n");
    out
}
