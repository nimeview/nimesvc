use crate::generators::common::errors::missing_header_status;
use crate::generators::common::headers::header_runtime_key;
use crate::generators::common::validation as common_validation;
use crate::ir::{Route, Service, Type, Validation};

use super::util::{
    body_struct_name, handler_struct_base, headers_struct_name, path_struct_name,
    query_struct_name, ts_type,
};

pub(crate) fn build_types_ts(service: &Service) -> String {
    let mut out = String::new();
    for en in &service.schema.enums {
        let is_numeric = en.variants.iter().all(|v| v.value.is_some());
        let enum_name = en.name.code_name();
        let variants = if is_numeric {
            en.variants
                .iter()
                .map(|v| v.value.unwrap().to_string())
                .collect::<Vec<_>>()
                .join(" | ")
        } else {
            en.variants
                .iter()
                .map(|v| format!("\"{}\"", v.name))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        out.push_str(&format!("export type {} = {};\n\n", enum_name, variants));
    }
    let mut needs_bool = false;
    let mut needs_validation = false;
    for ty in &service.schema.types {
        out.push_str(&format!("export interface {} {{\n", ty.name.code_name()));
        for field in &ty.fields {
            out.push_str(&format!("  {}: {};\n", field.name, ts_type(&field.ty)));
            if matches!(field.ty, Type::Bool) {
                needs_bool = true;
            }
        }
        out.push_str("}\n\n");
    }
    for ev in &service.events.definitions {
        let alias = format!("{}Event", ev.name.code_name());
        let payload = ts_type(&ev.payload);
        out.push_str(&format!("export type {} = {};\n\n", alias, payload));
    }
    if !service.sockets.sockets.is_empty() {
        out.push_str("export interface JoinPayload { user_id?: string; meta?: any; }\n");
        out.push_str("export interface ExitPayload { reason?: string; }\n");
        out.push_str(
            "export interface MessageInPayload { id?: string; text?: string; data?: any; }\n",
        );
        out.push_str(
            "export interface MessageOutPayload { id?: string; text?: string; data?: any; }\n",
        );
        out.push_str("export interface TypingPayload { user_id?: string; typing?: boolean; }\n");
        out.push_str("export interface PingPayload { ts?: number; }\n");
        out.push_str("export interface PongPayload { ts?: number; }\n");
        out.push_str("export interface AuthPayload { token?: string; }\n");
        out.push_str("export interface SubscribePayload { topic?: string; }\n");
        out.push_str("export interface UnsubscribePayload { topic?: string; }\n");
        out.push_str("export interface RoomJoinPayload { room?: string; }\n");
        out.push_str("export interface RoomLeavePayload { room?: string; }\n");
        out.push_str("export interface AckPayload { id?: string; }\n");
        out.push_str("export interface ReceiptPayload { id?: string; status?: string; }\n");
        out.push_str("export interface UserJoinedPayload { user_id?: string; }\n");
        out.push_str("export interface UserLeftPayload { user_id?: string; }\n");
        out.push_str("export interface ErrorPayload { message?: string; code?: string; }\n");
        out.push_str(
            "export interface ServerNoticePayload { message?: string; level?: string; }\n\n",
        );
    }
    for route in &service.http.routes {
        out.push_str(&render_input_types(route));
        for field in route
            .input
            .path
            .iter()
            .chain(route.input.query.iter())
            .chain(route.input.body.iter())
            .chain(route.headers.iter())
        {
            if matches!(field.ty, Type::Bool) {
                needs_bool = true;
            }
            if common_validation::field_needs_validation(field) {
                needs_validation = true;
            }
        }
    }
    if needs_bool {
        out.push_str(
            r#"function parseBool(value: any): boolean {
  if (typeof value === "boolean") return value;
  if (typeof value === "string") return value.toLowerCase() === "true";
  if (typeof value === "number") return value !== 0;
  return Boolean(value);
}

"#,
        );
    }
    if needs_validation {
        out.push_str(
            r#"function validateField<T>(value: T, invalid: (v: any) => boolean, label: string): T {
  if (value === undefined || value === null) return value;
  if (invalid(value)) {
    throw new Error(`Invalid ${label}`);
  }
  return value;
}

"#,
        );
    }
    out.push_str("export {};\n");
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InputKind {
    Path,
    Query,
    Body,
    Headers,
}

fn render_input_types(route: &Route) -> String {
    let mut out = String::new();
    if !route.input.path.is_empty() {
        out.push_str(&format!("export type {} = {{\n", path_struct_name(route)));
        for field in &route.input.path {
            out.push_str(&format!("  {}: {};\n", field.name, ts_type(&field.ty)));
        }
        out.push_str("};\n\n");
        out.push_str(&render_parser(route, InputKind::Path));
    }
    if !route.input.query.is_empty() {
        out.push_str(&format!("export type {} = {{\n", query_struct_name(route)));
        for field in &route.input.query {
            out.push_str(&format!("  {}?: {};\n", field.name, ts_type(&field.ty)));
        }
        out.push_str("};\n\n");
        out.push_str(&render_parser(route, InputKind::Query));
    }
    if !route.input.body.is_empty() {
        out.push_str(&format!("export type {} = {{\n", body_struct_name(route)));
        for field in &route.input.body {
            out.push_str(&format!("  {}: {};\n", field.name, ts_type(&field.ty)));
        }
        out.push_str("};\n\n");
        out.push_str(&render_parser(route, InputKind::Body));
    }
    if !route.headers.is_empty() {
        out.push_str(&format!(
            "export type {} = {{\n",
            headers_struct_name(route)
        ));
        for field in &route.headers {
            let opt = if field.optional { "?" } else { "" };
            out.push_str(&format!(
                "  {}{}: {};\n",
                field.name,
                opt,
                ts_type(&field.ty)
            ));
        }
        out.push_str("};\n\n");
        out.push_str(&render_parser(route, InputKind::Headers));
    }
    out
}

fn render_parser(route: &Route, kind: InputKind) -> String {
    let (struct_name, src_expr) = match kind {
        InputKind::Path => (path_struct_name(route), "req.params"),
        InputKind::Query => (query_struct_name(route), "req.query"),
        InputKind::Body => (body_struct_name(route), "req.body"),
        InputKind::Headers => (headers_struct_name(route), "req.headers"),
    };
    let func_name = match kind {
        InputKind::Path => format!("parsePath{}", handler_struct_base(route)),
        InputKind::Query => format!("parseQuery{}", handler_struct_base(route)),
        InputKind::Body => format!("parseBody{}", handler_struct_base(route)),
        InputKind::Headers => format!("parseHeaders{}", handler_struct_base(route)),
    };
    let fields = match kind {
        InputKind::Path => &route.input.path,
        InputKind::Query => &route.input.query,
        InputKind::Body => &route.input.body,
        InputKind::Headers => &route.headers,
    };

    let mut lines = Vec::new();
    lines.push(format!(
        "export function {}(req: any): {} {{",
        func_name, struct_name
    ));
    lines.push(format!("  const src: any = {} || {{}};", src_expr));
    lines.push("  const out: any = {};".to_string());
    for field in fields {
        let key = if kind == InputKind::Headers {
            header_runtime_key(&field.name)
        } else {
            field.name.clone()
        };
        if kind == InputKind::Headers && !field.optional {
            let status = missing_header_status(&field.name);
            lines.push(format!(
                "  if (src['{}'] === undefined) {{ throw {{ status: {}, message: 'missing header {}' }}; }}",
                key, status, field.name
            ));
        }
        let optional = if kind == InputKind::Headers {
            field.optional
        } else {
            field.optional || kind == InputKind::Query
        };
        let expr = parse_expr_for_type(
            &format!("src['{}']", key),
            &field.ty,
            optional,
            field.validation.as_ref(),
            &field.name,
        );
        lines.push(format!(
            "  out.{name} = {expr};",
            name = field.name,
            expr = expr
        ));
    }
    lines.push("  return out as any;".to_string());
    lines.push("}\n".to_string());
    lines.join("\n")
}

fn parse_expr_for_type(
    src: &str,
    ty: &Type,
    optional: bool,
    validation: Option<&Validation>,
    field_name: &str,
) -> String {
    let base = match ty {
        Type::Int => format!("parseInt({src} as any, 10)"),
        Type::Float => format!("parseFloat({src} as any)"),
        Type::Bool => format!("parseBool({src} as any)"),
        Type::String => format!("{src} as string"),
        Type::Array(_) => format!("Array.isArray({src}) ? {src} : []"),
        _ => format!("{src}"),
    };
    let mut expr = if optional {
        format!("({src} === undefined ? undefined : {base})")
    } else {
        base
    };
    if let Some(v) = validation {
        let mut checks = Vec::new();
        match ty {
            Type::String => {
                if let Some(min) = v.min_len.or(v.min) {
                    checks.push(format!("v.length < {}", min));
                }
                if let Some(max) = v.max_len.or(v.max) {
                    checks.push(format!("v.length > {}", max));
                }
                if let Some(regex) = &v.regex {
                    checks.push(format!("!new RegExp({:?}).test(v)", regex));
                } else if let Some(format) = &v.format {
                    if format == "email" {
                        checks.push(
                            "!new RegExp(\"^[^@\\\\s]+@[^@\\\\s]+\\\\.[^@\\\\s]+$\").test(v)"
                                .to_string(),
                        );
                    } else if format == "uuid" {
                        checks.push("!new RegExp(\"^[0-9a-fA-F-]{36}$\").test(v)".to_string());
                    }
                }
            }
            Type::Int | Type::Float => {
                if let Some(min) = v.min {
                    checks.push(format!("v < {}", min));
                }
                if let Some(max) = v.max {
                    checks.push(format!("v > {}", max));
                }
            }
            Type::Array(_) => {
                if let Some(min) = v.min_items.or(v.min) {
                    checks.push(format!("v.length < {}", min));
                }
                if let Some(max) = v.max_items.or(v.max) {
                    checks.push(format!("v.length > {}", max));
                }
            }
            _ => {}
        }
        if !checks.is_empty() {
            let cond = checks.join(" || ");
            expr = format!(
                "(validateField({expr}, (v) => {cond}, \"{field_name}\"))",
                field_name = field_name,
                expr = expr,
                cond = cond
            );
        }
    }
    expr
}
