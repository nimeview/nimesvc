use std::collections::BTreeMap;

use crate::ir::{AuthSpec, Field, Route, Service, Type, Validation, effective_auth};

use super::model::{
    Components, Info, MediaType, OpenApi, Operation, Parameter, RequestBody, Response, Schema,
    SecurityScheme,
};

pub fn generate_openapi(service: &Service) -> OpenApi {
    let mut paths: BTreeMap<String, BTreeMap<String, Operation>> = BTreeMap::new();

    for route in &service.http.routes {
        if route.internal {
            continue;
        }
        let operation = build_operation(route, service.common.auth.as_ref(), &service.http.headers);
        let method_key = route.method.as_openapi_key().to_string();
        paths
            .entry(route.path.clone())
            .or_default()
            .insert(method_key, operation);
    }

    let components = if service.schema.types.is_empty()
        && service.schema.enums.is_empty()
        && service.common.auth.is_none()
        && !service
            .http
            .routes
            .iter()
            .any(|r| r.auth.is_some() && !r.internal)
    {
        None
    } else {
        let mut schemas = BTreeMap::new();
        for ty in &service.schema.types {
            schemas.insert(ty.name.display(), object_schema_from_fields(&ty.fields));
        }
        for en in &service.schema.enums {
            let is_numeric = en.variants.iter().all(|v| v.value.is_some());
            let enum_values = if is_numeric {
                en.variants
                    .iter()
                    .map(|v| serde_json::Value::from(v.value.unwrap()))
                    .collect()
            } else {
                en.variants
                    .iter()
                    .map(|v| serde_json::Value::from(v.name.clone()))
                    .collect()
            };
            schemas.insert(
                en.name.display(),
                Schema {
                    ref_: None,
                    type_: Some(if is_numeric {
                        "integer".to_string()
                    } else {
                        "string".to_string()
                    }),
                    format: None,
                    items: None,
                    any_of: None,
                    one_of: None,
                    nullable: None,
                    properties: None,
                    required: None,
                    additional_properties: None,
                    minimum: None,
                    maximum: None,
                    min_length: None,
                    max_length: None,
                    pattern: None,
                    min_items: None,
                    max_items: None,
                    enum_values: Some(enum_values),
                    x_constraints: None,
                },
            );
        }

        let security_schemes = build_security_schemes(service);
        Some(Components {
            schemas,
            security_schemes,
        })
    };

    let events = if service.events.definitions.is_empty() {
        None
    } else {
        let mut map = BTreeMap::new();
        for ev in &service.events.definitions {
            map.insert(ev.name.display(), schema_from_type(&ev.payload));
        }
        Some(map)
    };

    OpenApi {
        openapi: "3.0.3".to_string(),
        info: Info {
            title: service.name.clone(),
            version: "0.1.0".to_string(),
        },
        paths,
        components,
        events,
    }
}

fn build_operation(
    route: &Route,
    service_auth: Option<&AuthSpec>,
    service_headers: &[Field],
) -> Operation {
    let mut parameters = Vec::new();
    for field in &route.input.path {
        parameters.push(Parameter {
            name: field.name.clone(),
            location: "path".to_string(),
            required: true,
            schema: schema_from_type_with_validation(&field.ty, field.validation.as_ref()),
        });
    }
    for field in &route.input.query {
        parameters.push(Parameter {
            name: field.name.clone(),
            location: "query".to_string(),
            required: !field.optional,
            schema: schema_from_type_with_validation(&field.ty, field.validation.as_ref()),
        });
    }

    let request_body = if route.input.body.is_empty() {
        None
    } else {
        let schema = object_schema_from_fields(&route.input.body);
        let mut content = BTreeMap::new();
        content.insert("application/json".to_string(), MediaType { schema });
        Some(RequestBody {
            required: true,
            content,
        })
    };

    for field in service_headers.iter().chain(route.headers.iter()) {
        parameters.push(Parameter {
            name: field.name.clone(),
            location: "header".to_string(),
            required: !field.optional,
            schema: schema_from_type_with_validation(&field.ty, field.validation.as_ref()),
        });
    }

    let mut responses = BTreeMap::new();
    for resp in &route.responses {
        match resp.ty {
            Type::Void => {
                let desc = if resp.status == 204 {
                    "No Content"
                } else {
                    "OK"
                };
                responses.insert(
                    resp.status.to_string(),
                    Response {
                        description: desc.to_string(),
                        headers: None,
                        content: None,
                    },
                );
            }
            _ => {
                let schema = schema_from_type_with_validation(&resp.ty, None);
                let mut content = BTreeMap::new();
                content.insert("application/json".to_string(), MediaType { schema });
                responses.insert(
                    resp.status.to_string(),
                    Response {
                        description: "OK".to_string(),
                        headers: None,
                        content: Some(content),
                    },
                );
            }
        }
    }

    Operation {
        parameters,
        request_body,
        security: build_operation_security(route, service_auth),
        responses,
    }
}

fn schema_from_type_with_validation(ty: &Type, validation: Option<&Validation>) -> Schema {
    let mut schema = schema_from_type(ty);
    if let Some(v) = validation {
        apply_validation(&mut schema, v, ty);
    }
    schema
}

fn schema_from_type(ty: &Type) -> Schema {
    match ty {
        Type::String => Schema {
            ref_: None,
            type_: Some("string".to_string()),
            format: None,
            items: None,
            any_of: None,
            one_of: None,
            nullable: None,
            properties: None,
            required: None,
            additional_properties: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
        Type::Int => Schema {
            ref_: None,
            type_: Some("integer".to_string()),
            format: Some("int64".to_string()),
            items: None,
            any_of: None,
            one_of: None,
            nullable: None,
            properties: None,
            required: None,
            additional_properties: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
        Type::Float => Schema {
            ref_: None,
            type_: Some("number".to_string()),
            format: Some("double".to_string()),
            items: None,
            any_of: None,
            one_of: None,
            nullable: None,
            properties: None,
            required: None,
            additional_properties: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
        Type::Bool => Schema {
            ref_: None,
            type_: Some("boolean".to_string()),
            format: None,
            items: None,
            any_of: None,
            one_of: None,
            nullable: None,
            properties: None,
            required: None,
            additional_properties: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
        Type::Object(fields) => object_schema_from_fields(fields),
        Type::Array(inner) => Schema {
            ref_: None,
            type_: Some("array".to_string()),
            format: None,
            items: Some(Box::new(schema_from_type(inner))),
            any_of: None,
            one_of: None,
            nullable: None,
            properties: None,
            required: None,
            additional_properties: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
        Type::Map(inner) => Schema {
            ref_: None,
            type_: Some("object".to_string()),
            format: None,
            items: None,
            any_of: None,
            one_of: None,
            nullable: None,
            properties: None,
            required: None,
            additional_properties: Some(Box::new(schema_from_type(inner))),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
        Type::Union(types) => Schema {
            ref_: None,
            type_: None,
            format: None,
            items: None,
            any_of: Some(types.iter().map(schema_from_type).collect()),
            one_of: None,
            nullable: None,
            properties: None,
            required: None,
            additional_properties: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
        Type::OneOf(types) => Schema {
            ref_: None,
            type_: None,
            format: None,
            items: None,
            any_of: None,
            one_of: Some(types.iter().map(schema_from_type).collect()),
            nullable: None,
            properties: None,
            required: None,
            additional_properties: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
        Type::Nullable(inner) => {
            let mut schema = schema_from_type(inner);
            schema.nullable = Some(true);
            schema
        }
        Type::Void => Schema {
            ref_: None,
            type_: Some("null".to_string()),
            format: None,
            items: None,
            any_of: None,
            one_of: None,
            nullable: None,
            properties: None,
            required: None,
            additional_properties: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
        Type::Any => Schema {
            ref_: None,
            type_: Some("object".to_string()),
            format: None,
            items: None,
            any_of: None,
            one_of: None,
            nullable: None,
            properties: None,
            required: None,
            additional_properties: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
        Type::Named(name) => Schema {
            ref_: Some(format!("#/components/schemas/{}", name.display())),
            type_: None,
            format: None,
            items: None,
            any_of: None,
            one_of: None,
            nullable: None,
            properties: None,
            required: None,
            additional_properties: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_items: None,
            max_items: None,
            enum_values: None,
            x_constraints: None,
        },
    }
}

fn object_schema_from_fields(fields: &[Field]) -> Schema {
    let mut props = BTreeMap::new();
    let mut required = Vec::new();
    for field in fields {
        let schema = schema_from_type_with_validation(&field.ty, field.validation.as_ref());
        props.insert(field.name.clone(), schema);
        if !field.optional {
            required.push(field.name.clone());
        }
    }
    Schema {
        ref_: None,
        type_: Some("object".to_string()),
        format: None,
        items: None,
        any_of: None,
        one_of: None,
        nullable: None,
        properties: Some(props),
        required: if required.is_empty() {
            None
        } else {
            Some(required)
        },
        additional_properties: None,
        minimum: None,
        maximum: None,
        min_length: None,
        max_length: None,
        pattern: None,
        min_items: None,
        max_items: None,
        enum_values: None,
        x_constraints: None,
    }
}

fn apply_validation(schema: &mut Schema, v: &Validation, ty: &Type) {
    match ty {
        Type::Nullable(inner) => {
            apply_validation(schema, v, inner);
        }
        Type::String => {
            schema.min_length = v.min_len.or(v.min);
            schema.max_length = v.max_len.or(v.max);
            schema.pattern = v.regex.clone();
            schema.format = v.format.clone().or(schema.format.clone());
        }
        Type::Int | Type::Float => {
            schema.minimum = v.min;
            schema.maximum = v.max;
            schema.format = v.format.clone().or(schema.format.clone());
        }
        Type::Array(_) => {
            schema.min_items = v.min_items.or(v.min);
            schema.max_items = v.max_items.or(v.max);
        }
        _ => {}
    }
    if !v.constraints.is_empty() {
        let mut map = BTreeMap::new();
        for (key, value) in &v.constraints {
            map.insert(key.clone(), serde_json::Value::from(value.clone()));
        }
        schema.x_constraints = Some(map);
    }
}

fn build_security_schemes(service: &Service) -> Option<BTreeMap<String, SecurityScheme>> {
    let mut schemes = BTreeMap::new();
    let mut add_scheme = |auth: &AuthSpec| match auth {
        AuthSpec::None => {}
        AuthSpec::Bearer => {
            schemes.insert(
                "BearerAuth".to_string(),
                SecurityScheme {
                    type_: "http".to_string(),
                    scheme: Some("bearer".to_string()),
                    name: None,
                    location: None,
                },
            );
        }
        AuthSpec::ApiKey => {
            schemes.insert(
                "ApiKeyAuth".to_string(),
                SecurityScheme {
                    type_: "apiKey".to_string(),
                    scheme: None,
                    name: Some("X-API-Key".to_string()),
                    location: Some("header".to_string()),
                },
            );
        }
    };
    if let Some(auth) = &service.common.auth {
        add_scheme(auth);
    }
    for route in &service.http.routes {
        if route.internal {
            continue;
        }
        if let Some(auth) = &route.auth {
            add_scheme(auth);
        }
    }
    if schemes.is_empty() {
        None
    } else {
        Some(schemes)
    }
}

fn build_operation_security(
    route: &Route,
    service_auth: Option<&AuthSpec>,
) -> Option<Vec<BTreeMap<String, Vec<String>>>> {
    let auth = effective_auth(route.auth.as_ref(), service_auth);
    let Some(auth) = auth else {
        return None;
    };
    let mut req = BTreeMap::new();
    match auth {
        AuthSpec::Bearer => {
            req.insert("BearerAuth".to_string(), Vec::new());
        }
        AuthSpec::ApiKey => {
            req.insert("ApiKeyAuth".to_string(), Vec::new());
        }
        AuthSpec::None => {
            return None;
        }
    }
    Some(vec![req])
}
