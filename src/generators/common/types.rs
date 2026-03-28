use crate::ir::Type;

pub fn uses_named_type(ty: &Type) -> bool {
    match ty {
        Type::Named(_) => true,
        Type::Array(inner) => uses_named_type(inner),
        Type::Object(fields) => fields.iter().any(|f| uses_named_type(&f.ty)),
        Type::Map(inner) => uses_named_type(inner),
        Type::Union(items) | Type::OneOf(items) => items.iter().any(uses_named_type),
        Type::Nullable(inner) => uses_named_type(inner),
        _ => false,
    }
}
