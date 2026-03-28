use crate::ir::Service;

use super::util::{go_field_name, go_type, wrap_optional};

pub(crate) fn build_types_go(service: &Service) -> String {
    build_types_go_with_package(service, "types")
}

pub(crate) fn build_types_go_with_package(service: &Service, package: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("package {}\n\n", package));

    for en in &service.schema.enums {
        let is_numeric = en.variants.iter().all(|v| v.value.is_some());
        let enum_name = en.name.code_name();
        if is_numeric {
            out.push_str(&format!("type {} int64\n", enum_name));
            out.push_str("const (\n");
            for var in &en.variants {
                let val = var.value.unwrap_or(0);
                out.push_str(&format!("    {} {} = {}\n", var.name, enum_name, val));
            }
            out.push_str(")\n\n");
        } else {
            out.push_str(&format!("type {} string\n", enum_name));
            out.push_str("const (\n");
            for var in &en.variants {
                out.push_str(&format!(
                    "    {} {} = \"{}\"\n",
                    var.name, enum_name, var.name
                ));
            }
            out.push_str(")\n\n");
        }
    }

    for ty in &service.schema.types {
        out.push_str(&format!("type {} struct {{\n", ty.name.code_name()));
        for field in &ty.fields {
            let go_name = go_field_name(&field.name);
            let go_ty = go_type(&field.ty, false);
            if field.optional {
                out.push_str(&format!(
                    "    {} {} `json:\"{},omitempty\"`\n",
                    go_name,
                    wrap_optional(&go_ty),
                    field.name
                ));
            } else {
                out.push_str(&format!(
                    "    {} {} `json:\"{}\"`\n",
                    go_name, go_ty, field.name
                ));
            }
        }
        out.push_str("}\n\n");
    }

    for ev in &service.events.definitions {
        let alias = format!("{}Event", ev.name.code_name());
        let payload_ty = go_type(&ev.payload, false);
        out.push_str(&format!("type {} = {}\n\n", alias, payload_ty));
    }

    if !service.sockets.sockets.is_empty() {
        out.push_str("type JoinPayload struct {\n    UserId *string `json:\"user_id,omitempty\"`\n    Meta any `json:\"meta,omitempty\"`\n}\n\n");
        out.push_str(
            "type ExitPayload struct {\n    Reason *string `json:\"reason,omitempty\"`\n}\n\n",
        );
        out.push_str("type MessageInPayload struct {\n    Id *string `json:\"id,omitempty\"`\n    Text *string `json:\"text,omitempty\"`\n    Data any `json:\"data,omitempty\"`\n}\n\n");
        out.push_str("type MessageOutPayload struct {\n    Id *string `json:\"id,omitempty\"`\n    Text *string `json:\"text,omitempty\"`\n    Data any `json:\"data,omitempty\"`\n}\n\n");
        out.push_str("type TypingPayload struct {\n    UserId *string `json:\"user_id,omitempty\"`\n    Typing *bool `json:\"typing,omitempty\"`\n}\n\n");
        out.push_str("type PingPayload struct {\n    Ts *int64 `json:\"ts,omitempty\"`\n}\n\n");
        out.push_str("type PongPayload struct {\n    Ts *int64 `json:\"ts,omitempty\"`\n}\n\n");
        out.push_str(
            "type AuthPayload struct {\n    Token *string `json:\"token,omitempty\"`\n}\n\n",
        );
        out.push_str(
            "type SubscribePayload struct {\n    Topic *string `json:\"topic,omitempty\"`\n}\n\n",
        );
        out.push_str(
            "type UnsubscribePayload struct {\n    Topic *string `json:\"topic,omitempty\"`\n}\n\n",
        );
        out.push_str(
            "type RoomJoinPayload struct {\n    Room *string `json:\"room,omitempty\"`\n}\n\n",
        );
        out.push_str(
            "type RoomLeavePayload struct {\n    Room *string `json:\"room,omitempty\"`\n}\n\n",
        );
        out.push_str("type AckPayload struct {\n    Id *string `json:\"id,omitempty\"`\n}\n\n");
        out.push_str("type ReceiptPayload struct {\n    Id *string `json:\"id,omitempty\"`\n    Status *string `json:\"status,omitempty\"`\n}\n\n");
        out.push_str("type UserJoinedPayload struct {\n    UserId *string `json:\"user_id,omitempty\"`\n}\n\n");
        out.push_str(
            "type UserLeftPayload struct {\n    UserId *string `json:\"user_id,omitempty\"`\n}\n\n",
        );
        out.push_str("type ErrorPayload struct {\n    Message *string `json:\"message,omitempty\"`\n    Code *string `json:\"code,omitempty\"`\n}\n\n");
        out.push_str("type ServerNoticePayload struct {\n    Message *string `json:\"message,omitempty\"`\n    Level *string `json:\"level,omitempty\"`\n}\n\n");
    }

    out
}
