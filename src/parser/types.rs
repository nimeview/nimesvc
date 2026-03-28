use anyhow::{Result, anyhow, bail};

use crate::ir::{EnumDef, EnumVariant, Field, Type, TypeDef, TypeName, Validation};

use super::model::{EnumBuilder, TypeBuilder};
use super::util::{extract_braced, is_ident, split_top_level_commas};

pub(super) fn parse_type_decl(content: &str, line_no: usize) -> Result<TypeBuilder> {
    let content = content.trim();
    let after = if let Some(rest) = content.strip_prefix("type ") {
        rest.trim()
    } else if let Some(rest) = content.strip_prefix("contract ") {
        rest.trim()
    } else {
        bail!(
            "Line {}: expected `type <Name>:` or `contract <Name>:`",
            line_no
        );
    };
    if after.contains('=') {
        let mut parts = after.splitn(2, '=');
        let name = parts.next().unwrap().trim();
        let rhs = parts.next().unwrap().trim();
        let name = parse_name_with_version(name, line_no)?;
        let ty = parse_type_expr(rhs, line_no)?;
        let fields = match ty {
            Type::Object(fields) => fields,
            _ => bail!("Line {}: type must be an object", line_no),
        };
        return Ok(TypeBuilder {
            name,
            fields,
            sealed: true,
        });
    }
    if !content.ends_with(':') {
        bail!(
            "Line {}: expected `type <Name>:` or `contract <Name>:`",
            line_no
        );
    }
    let name = after
        .strip_suffix(':')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid type name", line_no))?;
    let name = parse_name_with_version(name, line_no)?;

    Ok(TypeBuilder {
        name,
        fields: Vec::new(),
        sealed: false,
    })
}

pub(super) fn parse_type_field_line(
    ty: &mut TypeBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let content = content.trim();
    let mut parts = content.splitn(2, ':');
    let raw_name = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: missing field name", line_no))?;
    let (name, optional) = parse_optional_name(raw_name, line_no)?;
    let ty_raw = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: missing field type", line_no))?;
    let (field_ty, validation) = parse_type_with_validation(ty_raw, line_no)?;
    ty.fields.push(Field {
        name,
        ty: field_ty,
        optional,
        validation,
    });
    Ok(())
}

pub(super) fn parse_enum_decl(content: &str, line_no: usize) -> Result<EnumBuilder> {
    let content = content.trim();
    if !content.starts_with("enum ") || !content.ends_with(':') {
        bail!("Line {}: expected `enum <Name>:`", line_no);
    }
    let name = content
        .strip_prefix("enum ")
        .and_then(|s| s.strip_suffix(':'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid enum name", line_no))?;
    let name = parse_name_with_version(name, line_no)?;
    Ok(EnumBuilder {
        name,
        variants: Vec::new(),
    })
}

pub(super) fn parse_enum_variant_line(
    en: &mut EnumBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let content = content.trim();
    let mut parts = content.splitn(2, '=');
    let name = parts.next().unwrap().trim();
    if !is_ident(name) {
        bail!("Line {}: invalid enum variant '{}'", line_no, name);
    }
    let value = parts.next().map(|v| v.trim()).filter(|s| !s.is_empty());
    let value = if let Some(v) = value {
        Some(
            v.parse::<i64>()
                .map_err(|_| anyhow!("Line {}: invalid enum value", line_no))?,
        )
    } else {
        None
    };
    en.variants.push(EnumVariant {
        name: name.to_string(),
        value,
    });
    Ok(())
}

pub(super) fn finalize_type(ty: Option<TypeBuilder>, line_no: usize) -> Result<Option<TypeDef>> {
    let Some(ty) = ty else {
        return Ok(None);
    };
    if ty.fields.is_empty() {
        bail!(
            "Line {}: type '{}' has no fields",
            line_no,
            ty.name.display()
        );
    }
    Ok(Some(TypeDef {
        name: ty.name,
        fields: ty.fields,
    }))
}

pub(super) fn finalize_enum(en: Option<EnumBuilder>, line_no: usize) -> Result<Option<EnumDef>> {
    let Some(en) = en else {
        return Ok(None);
    };
    if en.variants.is_empty() {
        bail!(
            "Line {}: enum '{}' has no variants",
            line_no,
            en.name.display()
        );
    }
    Ok(Some(EnumDef {
        name: en.name,
        variants: en.variants,
    }))
}

pub(super) fn validate_named_types(
    types: &[TypeDef],
    enums: &[EnumDef],
    routes: &[crate::ir::Route],
    headers: &[crate::ir::Field],
    rpcs: &[crate::ir::RpcDef],
    sockets: &[crate::ir::SocketDef],
) -> Result<()> {
    let mut known: std::collections::HashSet<TypeName> = std::collections::HashSet::new();
    for ty in types {
        if !known.insert(ty.name.clone()) {
            bail!("Duplicate type name '{}'", ty.name.display());
        }
    }
    for en in enums {
        if !known.insert(en.name.clone()) {
            bail!("Duplicate type name '{}'", en.name.display());
        }
    }

    for ty in types {
        let ctx = ty.name.display();
        for field in &ty.fields {
            validate_type_ref(&field.ty, &known, &ctx)?;
        }
    }

    for field in headers {
        validate_type_ref(&field.ty, &known, "service headers")?;
    }

    for route in routes {
        for field in route
            .input
            .path
            .iter()
            .chain(route.input.query.iter())
            .chain(route.input.body.iter())
            .chain(route.headers.iter())
        {
            validate_type_ref(&field.ty, &known, "route input")?;
        }
        for resp in &route.responses {
            validate_type_ref(&resp.ty, &known, "route response")?;
        }
    }

    for rpc in rpcs {
        let ctx = format!("rpc {}.{}", rpc.service, rpc.name);
        for field in &rpc.input {
            validate_type_ref(&field.ty, &known, &ctx)?;
        }
        for field in &rpc.headers {
            validate_type_ref(&field.ty, &known, &ctx)?;
        }
        validate_type_ref(&rpc.output, &known, &ctx)?;
    }

    for socket in sockets {
        let ctx = format!("socket {}", socket.name.display());
        for field in &socket.headers {
            validate_type_ref(&field.ty, &known, &ctx)?;
        }
        for trigger in &socket.triggers {
            validate_type_ref(&trigger.payload, &known, &ctx)?;
        }
    }

    Ok(())
}

fn validate_type_ref(
    ty: &Type,
    known: &std::collections::HashSet<TypeName>,
    ctx: &str,
) -> Result<()> {
    match ty {
        Type::Named(name) => {
            if !known.contains(name) {
                bail!("Unknown type '{}' in {}", name.display(), ctx);
            }
        }
        Type::Array(inner) => validate_type_ref(inner, known, ctx)?,
        Type::Map(inner) => validate_type_ref(inner, known, ctx)?,
        Type::Union(items) | Type::OneOf(items) => {
            for item in items {
                validate_type_ref(item, known, ctx)?;
            }
        }
        Type::Nullable(inner) => validate_type_ref(inner, known, ctx)?,
        Type::Object(fields) => {
            for field in fields {
                validate_type_ref(&field.ty, known, ctx)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn parse_type_with_validation(
    raw: &str,
    line_no: usize,
) -> Result<(Type, Option<Validation>)> {
    let (type_part, validation_part) = split_validation(raw)?;
    let ty = parse_type_expr(type_part.trim(), line_no)?;
    let validation = if let Some(v) = validation_part {
        Some(parse_validation(v, line_no)?)
    } else {
        None
    };
    Ok((ty, validation))
}

pub(super) fn parse_optional_name(raw: &str, line_no: usize) -> Result<(String, bool)> {
    let name = raw.trim();
    if let Some(stripped) = name.strip_suffix('?') {
        if !is_ident(stripped) {
            bail!("Line {}: invalid field name '{}'", line_no, name);
        }
        Ok((stripped.to_string(), true))
    } else {
        if !is_ident(name) {
            bail!("Line {}: invalid field name '{}'", line_no, name);
        }
        Ok((name.to_string(), false))
    }
}

fn split_validation(raw: &str) -> Result<(&str, Option<&str>)> {
    let mut depth_angle = 0i32;
    let mut depth_brace = 0i32;
    let mut depth_paren = 0i32;
    for (i, ch) in raw.char_indices().rev() {
        match ch {
            ')' => depth_paren += 1,
            '(' => {
                depth_paren -= 1;
                if depth_paren == 0 && depth_angle == 0 && depth_brace == 0 {
                    let type_part = &raw[..i];
                    let val_part = &raw[i + 1..raw.len() - 1];
                    return Ok((type_part, Some(val_part)));
                }
            }
            '>' => depth_angle += 1,
            '<' => depth_angle -= 1,
            '}' => depth_brace += 1,
            '{' => depth_brace -= 1,
            _ => {}
        }
    }
    Ok((raw, None))
}

fn parse_validation(raw: &str, line_no: usize) -> Result<Validation> {
    let mut v = Validation {
        min: None,
        max: None,
        min_len: None,
        max_len: None,
        min_items: None,
        max_items: None,
        regex: None,
        format: None,
        constraints: std::collections::BTreeMap::new(),
    };
    for part in raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let mut kv = part.splitn(2, '=');
        let key = kv.next().unwrap().trim();
        let val = kv
            .next()
            .ok_or_else(|| anyhow!("Line {}: invalid validation", line_no))?
            .trim();
        match key {
            "min" => {
                v.min = Some(
                    val.parse::<i64>()
                        .map_err(|_| anyhow!("Line {}: invalid min", line_no))?,
                )
            }
            "max" => {
                v.max = Some(
                    val.parse::<i64>()
                        .map_err(|_| anyhow!("Line {}: invalid max", line_no))?,
                )
            }
            "min_len" | "min_length" => {
                v.min_len = Some(
                    val.parse::<i64>()
                        .map_err(|_| anyhow!("Line {}: invalid min_len", line_no))?,
                )
            }
            "max_len" | "max_length" => {
                v.max_len = Some(
                    val.parse::<i64>()
                        .map_err(|_| anyhow!("Line {}: invalid max_len", line_no))?,
                )
            }
            "min_items" => {
                v.min_items = Some(
                    val.parse::<i64>()
                        .map_err(|_| anyhow!("Line {}: invalid min_items", line_no))?,
                )
            }
            "max_items" => {
                v.max_items = Some(
                    val.parse::<i64>()
                        .map_err(|_| anyhow!("Line {}: invalid max_items", line_no))?,
                )
            }
            "regex" | "pattern" => {
                let val = val.trim_matches('"');
                v.regex = Some(val.to_string());
            }
            "format" => {
                let val = val.trim_matches('"');
                v.format = Some(val.to_string());
            }
            "email" => v.format = Some("email".to_string()),
            "uuid" => v.format = Some("uuid".to_string()),
            "len" | "length" => {
                let val = val
                    .parse::<i64>()
                    .map_err(|_| anyhow!("Line {}: invalid len", line_no))?;
                v.min_len = Some(val);
                v.max_len = Some(val);
            }
            _ => {
                v.constraints
                    .insert(key.to_string(), val.trim_matches('"').to_string());
            }
        }
    }
    Ok(v)
}

pub(super) fn parse_type_expr(raw: &str, line_no: usize) -> Result<Type> {
    let raw = raw.trim();
    if raw.starts_with("object") && raw.contains('{') {
        let start = raw.find('{').unwrap();
        let inner = extract_braced(&raw[start..])?;
        let fields = parse_inline_fields(inner, line_no)?;
        return Ok(Type::Object(fields));
    }
    if raw.starts_with('{') {
        let inner = extract_braced(raw)?;
        let fields = parse_inline_fields(inner, line_no)?;
        return Ok(Type::Object(fields));
    }
    if let Some(inner) = strip_generic(raw, "nullable") {
        let inner_ty = parse_type_expr(inner, line_no)?;
        return Ok(Type::Nullable(Box::new(inner_ty)));
    }
    if let Some(inner) = strip_generic(raw, "map") {
        let parts = split_top_level_commas(inner);
        if parts.is_empty() || parts.len() > 2 {
            bail!(
                "Line {}: map expects map<value> or map<string, value>",
                line_no
            );
        }
        if parts.len() == 2 {
            let key = parts[0].trim();
            if key != "string" {
                bail!("Line {}: map key must be string", line_no);
            }
            let val_ty = parse_type_expr(parts[1].trim(), line_no)?;
            return Ok(Type::Map(Box::new(val_ty)));
        }
        let val_ty = parse_type_expr(parts[0].trim(), line_no)?;
        return Ok(Type::Map(Box::new(val_ty)));
    }
    if let Some(inner) = strip_generic(raw, "union") {
        let parts = split_top_level_commas(inner);
        if parts.len() < 2 {
            bail!("Line {}: union requires 2+ types", line_no);
        }
        let mut types = Vec::new();
        for part in parts {
            types.push(parse_type_expr(part.trim(), line_no)?);
        }
        return Ok(Type::Union(types));
    }
    if let Some(inner) = strip_generic(raw, "oneof") {
        let parts = split_top_level_commas(inner);
        if parts.len() < 2 {
            bail!("Line {}: oneOf requires 2+ types", line_no);
        }
        let mut types = Vec::new();
        for part in parts {
            types.push(parse_type_expr(part.trim(), line_no)?);
        }
        return Ok(Type::OneOf(types));
    }
    if let Some(inner) = raw.strip_prefix("array<").and_then(|s| s.strip_suffix('>')) {
        let inner_ty = parse_type_expr(inner, line_no)?;
        return Ok(Type::Array(Box::new(inner_ty)));
    }
    parse_type(raw, line_no)
}

fn parse_inline_fields(raw: &str, line_no: usize) -> Result<Vec<Field>> {
    let mut fields = Vec::new();
    for part in split_top_level_commas(raw) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut kv = part.splitn(2, ':');
        let raw_name = kv
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("Line {}: invalid object field", line_no))?;
        let (name, optional) = parse_optional_name(raw_name, line_no)?;
        let raw_type = kv
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("Line {}: invalid object field", line_no))?;
        let (ty, validation) = parse_type_with_validation(raw_type, line_no)?;
        fields.push(Field {
            name,
            ty,
            optional,
            validation,
        });
    }
    Ok(fields)
}

fn strip_generic<'a>(raw: &'a str, name: &str) -> Option<&'a str> {
    let raw = raw.trim();
    let raw_lower = raw.to_ascii_lowercase();
    let prefix = format!("{name}<");
    if raw_lower.starts_with(&prefix) && raw.ends_with('>') {
        let inner = &raw[prefix.len()..raw.len() - 1];
        return Some(inner.trim());
    }
    None
}

pub(super) fn parse_type(raw: &str, line_no: usize) -> Result<Type> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("string") {
        return Ok(Type::String);
    }
    if raw.eq_ignore_ascii_case("int") || raw.eq_ignore_ascii_case("integer") {
        return Ok(Type::Int);
    }
    if raw.eq_ignore_ascii_case("float") || raw.eq_ignore_ascii_case("number") {
        return Ok(Type::Float);
    }
    if raw.eq_ignore_ascii_case("bool") || raw.eq_ignore_ascii_case("boolean") {
        return Ok(Type::Bool);
    }
    if raw.eq_ignore_ascii_case("object") {
        return Ok(Type::Object(Vec::new()));
    }
    if raw.eq_ignore_ascii_case("void") {
        return Ok(Type::Void);
    }
    if raw.eq_ignore_ascii_case("any") {
        return Ok(Type::Any);
    }

    if let Some(inner) = raw.strip_prefix("array<").and_then(|s| s.strip_suffix('>')) {
        let inner_ty = parse_type(inner, line_no)?;
        return Ok(Type::Array(Box::new(inner_ty)));
    }
    if raw.eq_ignore_ascii_case("array") {
        return Ok(Type::Array(Box::new(Type::Any)));
    }

    if is_ident(raw) || raw.contains('@') {
        let name = parse_name_with_version(raw, line_no)?;
        return Ok(Type::Named(name));
    }

    bail!("Line {}: unsupported type '{}'", line_no, raw)
}

pub(super) fn parse_name_with_version(raw: &str, line_no: usize) -> Result<TypeName> {
    let raw = raw.trim();
    if let Some((name, ver)) = raw.split_once('@') {
        let name = name.trim();
        let ver = ver.trim();
        if name.is_empty() || ver.is_empty() {
            bail!("Line {}: invalid versioned name '{}'", line_no, raw);
        }
        if !is_ident(name) {
            bail!("Line {}: invalid type name '{}'", line_no, name);
        }
        let version: u32 = ver
            .parse()
            .map_err(|_| anyhow!("Line {}: invalid version '{}' in '{}'", line_no, ver, raw))?;
        return Ok(TypeName {
            name: name.to_string(),
            version: Some(version),
        });
    }
    if !is_ident(raw) {
        bail!("Line {}: invalid type name '{}'", line_no, raw);
    }
    Ok(TypeName {
        name: raw.to_string(),
        version: None,
    })
}
