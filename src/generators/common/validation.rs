use crate::ir::{Field, Route, Type, Validation};

pub fn field_needs_validation(field: &Field) -> bool {
    field.validation.is_some()
        || matches!(
            field.ty,
            Type::Map(_) | Type::Union(_) | Type::OneOf(_) | Type::Nullable(_)
        )
}

pub fn route_has_validation(route: &Route) -> bool {
    route
        .input
        .path
        .iter()
        .chain(route.input.query.iter())
        .chain(route.input.body.iter())
        .chain(route.headers.iter())
        .any(field_needs_validation)
}

pub fn route_has_regex(route: &Route) -> bool {
    route
        .input
        .path
        .iter()
        .chain(route.input.query.iter())
        .chain(route.input.body.iter())
        .chain(route.headers.iter())
        .any(|f| {
            f.validation
                .as_ref()
                .map(|v| v.regex.is_some() || v.format.is_some())
                .unwrap_or(false)
        })
}

pub fn route_has_union_validation(route: &Route) -> bool {
    route
        .input
        .path
        .iter()
        .chain(route.input.query.iter())
        .chain(route.input.body.iter())
        .chain(route.headers.iter())
        .any(|f| type_needs_union_check(&f.ty, f.validation.as_ref()))
}

fn type_needs_union_check(ty: &Type, validation: Option<&Validation>) -> bool {
    if validation.is_some() {
        return true;
    }
    match ty {
        Type::Union(items) | Type::OneOf(items) => {
            items.iter().any(|i| type_needs_union_check(i, None))
        }
        Type::Nullable(inner) => type_needs_union_check(inner, None),
        Type::Array(inner) => type_needs_union_check(inner, None),
        Type::Map(inner) => type_needs_union_check(inner, None),
        Type::Object(fields) => fields
            .iter()
            .any(|f| type_needs_union_check(&f.ty, f.validation.as_ref())),
        _ => false,
    }
}
