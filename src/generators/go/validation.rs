use crate::generators::common::validation as common_validation;
use crate::ir::{Route, Service, Type};

use super::util::go_field_name;

pub(super) fn render_validation_helpers(service: &Service) -> String {
    if !service
        .http
        .routes
        .iter()
        .any(common_validation::route_has_union_validation)
    {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(
        r#"
func isString(v any) bool {
    _, ok := v.(string)
    return ok
}

func isBool(v any) bool {
    _, ok := v.(bool)
    return ok
}

func isNumber(v any) bool {
    switch v.(type) {
    case int, int64, float32, float64:
        return true
    default:
        return false
    }
}

func isInt(v any) bool {
    switch t := v.(type) {
    case int, int64:
        return true
    case float64:
        return math.Trunc(t) == t
    default:
        return false
    }
}

func isArray(v any) bool {
    _, ok := v.([]any)
    return ok
}

func isMap(v any) bool {
    _, ok := v.(map[string]any)
    return ok
}

"#,
    );
    out
}

pub(super) fn render_validation_block(route: &Route) -> String {
    if !common_validation::route_has_validation(route) {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&render_field_validations("path", &route.input.path, false));
    out.push_str(&render_field_validations("query", &route.input.query, true));
    out.push_str(&render_field_validations("body", &route.input.body, false));
    out.push_str(&render_field_validations("headers", &route.headers, false));
    out
}

fn render_field_validations(
    base: &str,
    fields: &[crate::ir::Field],
    optional_struct: bool,
) -> String {
    let mut out = String::new();
    for field in fields {
        if !common_validation::field_needs_validation(field) {
            continue;
        }
        let access = format!("{}.{}", base, go_field_name(&field.name));
        let name = field.name.clone();
        if optional_struct || field.optional {
            out.push_str(&format!("    if {access} != nil {{\n", access = access));
            let value_expr = match field.ty {
                Type::Nullable(_) => access.clone(),
                _ => format!("*{}", access),
            };
            out.push_str(&render_validation_checks(
                value_expr,
                &field.ty,
                field.validation.as_ref(),
                &name,
                8,
            ));
            out.push_str("    }\n");
        } else {
            out.push_str(&render_validation_checks(
                access,
                &field.ty,
                field.validation.as_ref(),
                &name,
                4,
            ));
        }
    }
    out
}

fn render_validation_checks(
    value_expr: String,
    ty: &Type,
    validation: Option<&crate::ir::Validation>,
    field_name: &str,
    indent: usize,
) -> String {
    let pad = " ".repeat(indent);
    let mut out = String::new();

    match ty {
        Type::Nullable(inner) => {
            out.push_str(&format!(
                "{pad}if {value} != nil {{\n",
                pad = pad,
                value = value_expr
            ));
            out.push_str(&render_validation_checks(
                "*".to_string() + &value_expr,
                inner,
                validation,
                field_name,
                indent + 4,
            ));
            out.push_str(&format!("{pad}}}\n", pad = pad));
            return out;
        }
        Type::Union(types) | Type::OneOf(types) => {
            let expr = render_go_match_expr(types, matches!(ty, Type::OneOf(_)), &value_expr);
            out.push_str(&format!(
                "{pad}if !{expr} {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                pad = pad,
                expr = expr,
                name = field_name
            ));
        }
        Type::Map(_) => {
            if let Some(v) = validation {
                let min = v.min_items.or(v.min);
                let max = v.max_items.or(v.max);
                if let Some(min) = min {
                    out.push_str(&format!(
                        "{pad}if len({value}) < {min} {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        min = min,
                        name = field_name
                    ));
                }
                if let Some(max) = max {
                    out.push_str(&format!(
                        "{pad}if len({value}) > {max} {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        max = max,
                        name = field_name
                    ));
                }
            }
            if type_needs_deep_check(ty) {
                out.push_str(&format!(
                    "{pad}for _, item := range {value} {{\n",
                    pad = pad,
                    value = value_expr
                ));
                out.push_str(&render_validation_checks(
                    "item".to_string(),
                    match ty {
                        Type::Map(inner) => inner,
                        _ => ty,
                    },
                    None,
                    field_name,
                    indent + 4,
                ));
                out.push_str(&format!("{pad}}}\n", pad = pad));
            }
        }
        _ => {}
    }

    if let Some(v) = validation {
        match ty {
            Type::String => {
                if let Some(min) = v.min_len.or(v.min) {
                    out.push_str(&format!(
                        "{pad}if len({value}) < {min} {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        min = min,
                        name = field_name
                    ));
                }
                if let Some(max) = v.max_len.or(v.max) {
                    out.push_str(&format!(
                        "{pad}if len({value}) > {max} {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        max = max,
                        name = field_name
                    ));
                }
                if let Some(regex) = &v.regex {
                    out.push_str(&format!(
                        "{pad}if ok, _ := regexp.MatchString({regex:?}, {value}); !ok {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        regex = regex,
                        value = value_expr,
                        name = field_name
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
                            "{pad}if ok, _ := regexp.MatchString({pattern:?}, {value}); !ok {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                            pad = pad,
                            pattern = pattern,
                            value = value_expr,
                            name = field_name
                        ));
                    }
                }
            }
            Type::Int => {
                if let Some(min) = v.min {
                    out.push_str(&format!(
                        "{pad}if {value} < {min} {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        min = min,
                        name = field_name
                    ));
                }
                if let Some(max) = v.max {
                    out.push_str(&format!(
                        "{pad}if {value} > {max} {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        max = max,
                        name = field_name
                    ));
                }
            }
            Type::Float => {
                if let Some(min) = v.min {
                    out.push_str(&format!(
                        "{pad}if {value} < float64({min}) {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        min = min,
                        name = field_name
                    ));
                }
                if let Some(max) = v.max {
                    out.push_str(&format!(
                        "{pad}if {value} > float64({max}) {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        max = max,
                        name = field_name
                    ));
                }
            }
            Type::Array(_) => {
                if let Some(min) = v.min_items.or(v.min) {
                    out.push_str(&format!(
                        "{pad}if len({value}) < {min} {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        min = min,
                        name = field_name
                    ));
                }
                if let Some(max) = v.max_items.or(v.max) {
                    out.push_str(&format!(
                        "{pad}if len({value}) > {max} {{\n{pad}    http.Error(w, \"invalid {name}\", http.StatusBadRequest)\n{pad}    return\n{pad}}}\n",
                        pad = pad,
                        value = value_expr,
                        max = max,
                        name = field_name
                    ));
                }
            }
            _ => {}
        }
    }
    if matches!(ty, Type::Array(_)) {
        if let Type::Array(inner) = ty {
            if type_needs_deep_check(inner) {
                out.push_str(&format!(
                    "{pad}for _, item := range {value} {{\n",
                    pad = pad,
                    value = value_expr
                ));
                out.push_str(&render_validation_checks(
                    "item".to_string(),
                    inner,
                    None,
                    field_name,
                    indent + 4,
                ));
                out.push_str(&format!("{pad}}}\n", pad = pad));
            }
        }
    }
    out
}

fn type_needs_deep_check(ty: &Type) -> bool {
    match ty {
        Type::Union(_) | Type::OneOf(_) | Type::Nullable(_) => true,
        Type::Array(inner) | Type::Map(inner) => type_needs_deep_check(inner),
        _ => false,
    }
}

fn render_go_match_expr(types: &[Type], oneof: bool, value_expr: &str) -> String {
    let exprs: Vec<String> = types
        .iter()
        .map(|ty| render_go_type_match_expr(ty, value_expr))
        .collect();
    if oneof {
        let mut lines = Vec::new();
        lines.push("func() bool {".to_string());
        lines.push("    count := 0".to_string());
        for expr in &exprs {
            lines.push(format!("    if {expr} {{ count++ }}", expr = expr));
        }
        lines.push("    return count == 1".to_string());
        lines.push("}()".to_string());
        lines.join("\n")
    } else {
        format!("({})", exprs.join(" || "))
    }
}

fn render_go_type_match_expr(ty: &Type, value_expr: &str) -> String {
    match ty {
        Type::String => format!("isString({})", value_expr),
        Type::Int => format!("isInt({})", value_expr),
        Type::Float => format!("isNumber({})", value_expr),
        Type::Bool => format!("isBool({})", value_expr),
        Type::Array(_) => format!("isArray({})", value_expr),
        Type::Map(_) => format!("isMap({})", value_expr),
        Type::Object(_) | Type::Named(_) => format!("isMap({})", value_expr),
        Type::Any => "true".to_string(),
        Type::Void => format!("{} == nil", value_expr),
        Type::Nullable(inner) => format!(
            "{} == nil || {}",
            value_expr,
            render_go_type_match_expr(inner, value_expr)
        ),
        Type::Union(types) => render_go_match_expr(types, false, value_expr),
        Type::OneOf(types) => render_go_match_expr(types, true, value_expr),
    }
}
