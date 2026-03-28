use crate::generators::common::validation as common_validation;
use crate::ir::{Field, Route, Service, Type, Validation};

pub(super) fn render_validation_helpers(service: &Service) -> String {
    let mut out = String::new();
    if !service.http.routes.iter().any(route_has_validation) {
        return out;
    }
    out.push_str(
        r#"
fn invalid_field(name: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, format!("invalid {}", name))
}
"#,
    );
    out
}

pub(super) fn render_validation_block(route: &Route) -> String {
    if !route_has_validation(route) {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&render_field_validations("path", &route.input.path, false));
    out.push_str(&render_field_validations("query", &route.input.query, true));
    out.push_str(&render_field_validations("body", &route.input.body, false));
    out.push_str(&render_field_validations("headers", &route.headers, false));
    if out.is_empty() { String::new() } else { out }
}

pub(super) fn route_has_validation(route: &Route) -> bool {
    common_validation::route_has_validation(route)
}

fn render_field_validations(base: &str, fields: &[Field], optional_struct: bool) -> String {
    let mut out = String::new();
    for field in fields {
        if !field_needs_validation(field) {
            continue;
        }
        let access = format!("{}.{}", base, field.name);
        let field_label = field.name.clone();
        if optional_struct || field.optional {
            out.push_str(&format!("    if let Some(value) = &{} {{\n", access));
            out.push_str(&render_validation_checks(
                "value",
                &field.ty,
                field.validation.as_ref(),
                &field_label,
                8,
            ));
            out.push_str("    }\n");
        } else {
            out.push_str(&render_validation_checks(
                &access,
                &field.ty,
                field.validation.as_ref(),
                &field_label,
                4,
            ));
        }
    }
    out
}

fn render_validation_checks(
    value_expr: &str,
    ty: &Type,
    validation: Option<&Validation>,
    field_label: &str,
    indent: usize,
) -> String {
    let pad = " ".repeat(indent);
    let mut out = String::new();
    match ty {
        Type::Nullable(inner) => {
            out.push_str(&format!(
                "{pad}if let Some(inner) = {value}.as_ref() {{\n",
                pad = pad,
                value = value_expr
            ));
            out.push_str(&render_validation_checks(
                "inner",
                inner,
                validation,
                field_label,
                indent + 4,
            ));
            out.push_str(&format!("{pad}}}\n", pad = pad));
            return out;
        }
        Type::Union(types) | Type::OneOf(types) => {
            let expr = render_json_match_expr(types, matches!(ty, Type::OneOf(_)), value_expr);
            out.push_str(&format!(
                "{pad}if !({expr}) {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                pad = pad,
                expr = expr,
                name = field_label
            ));
        }
        Type::Map(_) => {
            if let Some(v) = validation {
                let min = v.min_items.or(v.min);
                let max = v.max_items.or(v.max);
                if let Some(min) = min {
                    out.push_str(&format!(
                        "{pad}if {value}.len() < {min} as usize {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        min = min,
                        name = field_label
                    ));
                }
                if let Some(max) = max {
                    out.push_str(&format!(
                        "{pad}if {value}.len() > {max} as usize {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        max = max,
                        name = field_label
                    ));
                }
            }
            if type_needs_deep_check(ty) {
                out.push_str(&format!(
                    "{pad}for item in {value}.values() {{\n",
                    pad = pad,
                    value = value_expr
                ));
                out.push_str(&render_validation_checks(
                    "item",
                    match ty {
                        Type::Map(inner) => inner,
                        _ => ty,
                    },
                    None,
                    field_label,
                    indent + 4,
                ));
                out.push_str(&format!("{pad}}}\n", pad = pad));
            }
        }
        Type::Array(inner) => {
            if type_needs_deep_check(inner) {
                out.push_str(&format!(
                    "{pad}for item in {value}.iter() {{\n",
                    pad = pad,
                    value = value_expr
                ));
                out.push_str(&render_validation_checks(
                    "item",
                    inner,
                    None,
                    field_label,
                    indent + 4,
                ));
                out.push_str(&format!("{pad}}}\n", pad = pad));
            }
        }
        Type::Object(_) => {
            out.push_str(&format!(
                "{pad}if !{value}.is_object() {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                pad = pad,
                value = value_expr,
                name = field_label
            ));
        }
        _ => {}
    }

    if let Some(v) = validation {
        match ty {
            Type::String => {
                if let Some(min) = v.min_len.or(v.min) {
                    out.push_str(&format!(
                        "{pad}if {value}.len() < {min} as usize {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        min = min,
                        name = field_label
                    ));
                }
                if let Some(max) = v.max_len.or(v.max) {
                    out.push_str(&format!(
                        "{pad}if {value}.len() > {max} as usize {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        max = max,
                        name = field_label
                    ));
                }
                if let Some(regex) = &v.regex {
                    out.push_str(&format!(
                        "{pad}if !Regex::new({regex:?}).unwrap().is_match({value}) {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        regex = regex,
                        value = value_expr,
                        name = field_label
                    ));
                } else if let Some(format) = &v.format {
                    let pattern = match format.as_str() {
                        "email" => "^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$",
                        "uuid" => {
                            "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
                        }
                        _ => "",
                    };
                    if !pattern.is_empty() {
                        out.push_str(&format!(
                            "{pad}if !Regex::new({pattern:?}).unwrap().is_match({value}) {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                            pad = pad,
                            pattern = pattern,
                            value = value_expr,
                            name = field_label
                        ));
                    }
                }
            }
            Type::Int => {
                if let Some(min) = v.min {
                    out.push_str(&format!(
                        "{pad}if *{value} < {min} {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        min = min,
                        name = field_label
                    ));
                }
                if let Some(max) = v.max {
                    out.push_str(&format!(
                        "{pad}if *{value} > {max} {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        max = max,
                        name = field_label
                    ));
                }
            }
            Type::Float => {
                if let Some(min) = v.min {
                    out.push_str(&format!(
                        "{pad}if *{value} < {min} as f64 {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        min = min,
                        name = field_label
                    ));
                }
                if let Some(max) = v.max {
                    out.push_str(&format!(
                        "{pad}if *{value} > {max} as f64 {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        max = max,
                        name = field_label
                    ));
                }
            }
            Type::Array(_) => {
                if let Some(min) = v.min_items.or(v.min) {
                    out.push_str(&format!(
                        "{pad}if {value}.len() < {min} as usize {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        min = min,
                        name = field_label
                    ));
                }
                if let Some(max) = v.max_items.or(v.max) {
                    out.push_str(&format!(
                        "{pad}if {value}.len() > {max} as usize {{\n{pad}    return invalid_field(\"{name}\").into_response();\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        max = max,
                        name = field_label
                    ));
                }
            }
            _ => {}
        }
    }
    out
}

fn field_needs_validation(field: &Field) -> bool {
    common_validation::field_needs_validation(field)
}

fn render_json_match_expr(types: &[Type], oneof: bool, value_expr: &str) -> String {
    let mut parts = Vec::new();
    for ty in types {
        parts.push(json_match_expr(ty, value_expr));
    }
    if oneof {
        let mut lines = Vec::new();
        lines.push("{".to_string());
        lines.push("    let mut count = 0;".to_string());
        for expr in &parts {
            lines.push(format!("    if {expr} {{ count += 1; }}", expr = expr));
        }
        lines.push("    count == 1".to_string());
        lines.push("}".to_string());
        lines.join("\n")
    } else {
        format!("({})", parts.join(" || "))
    }
}

fn json_match_expr(ty: &Type, value_expr: &str) -> String {
    match ty {
        Type::String => format!("{value}.is_string()", value = value_expr),
        Type::Int => format!(
            "{value}.as_i64().is_some() || {value}.as_u64().is_some()",
            value = value_expr
        ),
        Type::Float => format!("{value}.is_number()", value = value_expr),
        Type::Bool => format!("{value}.is_boolean()", value = value_expr),
        Type::Array(_) => format!("{value}.is_array()", value = value_expr),
        Type::Map(_) => format!("{value}.is_object()", value = value_expr),
        Type::Object(_) | Type::Named(_) => format!("{value}.is_object()", value = value_expr),
        Type::Any => "true".to_string(),
        Type::Void => format!("{value}.is_null()", value = value_expr),
        Type::Nullable(inner) => format!(
            "{value}.is_null() || {}",
            json_match_expr(inner, value_expr),
            value = value_expr
        ),
        Type::Union(types) => render_json_match_expr(types, false, value_expr),
        Type::OneOf(types) => render_json_match_expr(types, true, value_expr),
    }
}

fn type_needs_deep_check(ty: &Type) -> bool {
    match ty {
        Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) | Type::Object(_) => true,
        Type::Array(inner) | Type::Map(inner) => type_needs_deep_check(inner),
        _ => false,
    }
}
