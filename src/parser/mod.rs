mod events;
mod model;
mod project;
mod route;
mod rpc;
mod service;
mod socket;
mod types;
mod use_decl;
mod util;

pub use project::parse_project;

#[cfg(test)]
mod tests {
    use super::parse_project;
    use crate::ir::{Type, TypeName};

    #[test]
    fn parse_minimal_service() {
        let src = r#"
service API:
    GET "/hello":
        response string
        call db.hello
"#;
        let proj = parse_project(src).unwrap();
        let svc = &proj.services[0];
        assert_eq!(svc.name, "API");
        assert_eq!(svc.http.routes.len(), 1);
        assert_eq!(svc.http.routes[0].path, "/hello");
    }

    #[test]
    fn parse_service_with_input() {
        let src = r#"
service API:
    POST "/users/{id}":
        input:
            path:
                id: int
            query:
                q: string
            body:
                name: string
        response string
        call db.create_user(path.id, query.q, body.name)
"#;
        let proj = parse_project(src).unwrap();
        let route = &proj.services[0].http.routes[0];
        assert_eq!(route.input.path.len(), 1);
        assert_eq!(route.input.query.len(), 1);
        assert_eq!(route.input.body.len(), 1);
    }

    #[test]
    fn parse_service_with_types() {
        let src = r#"
type User:
    id: int
    name: string

service API:
    GET "/users":
        response User
        call db.list_users
"#;
        let proj = parse_project(src).unwrap();
        let svc = &proj.services[0];
        assert_eq!(svc.schema.types.len(), 1);
        assert_eq!(
            svc.http.routes[0].responses[0].ty,
            Type::Named(TypeName {
                name: "User".to_string(),
                version: None
            })
        );
    }

    #[test]
    fn parse_output_and_use() {
        let src = r#"
output "./server"
use db "./modules/db.rs"

service API:
    address "127.0.0.1"
    port 4000
    GET "/hello":
        response string
        call db.hello
"#;
        let proj = parse_project(src).unwrap();
        assert_eq!(proj.common.output.as_deref(), Some("./server"));
        assert_eq!(proj.common.modules.len(), 1);
        assert_eq!(proj.common.modules[0].name, "db");
        assert_eq!(
            proj.services[0].common.address.as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(proj.services[0].common.port, Some(4000));
    }

    #[test]
    fn parse_use_with_alias() {
        let src = r#"
use db "./modules/db.rs" as storage

service API:
    GET "/hello":
        response string
        call db.hello
"#;
        let proj = parse_project(src).unwrap();
        assert_eq!(proj.common.modules[0].alias.as_deref(), Some("storage"));
    }

    #[test]
    fn parse_use_crate_only() {
        let src = r#"
use sqlx

service API:
    GET "/hello":
        response string
        call db.hello
"#;
        let proj = parse_project(src).unwrap();
        assert_eq!(proj.common.modules[0].name, "sqlx");
        assert!(proj.common.modules[0].path.is_none());
    }

    #[test]
    fn parse_use_versioned() {
        let src = r#"
use sqlx "0.7"

service API:
    GET "/hello":
        response string
        call db.hello
"#;
        let proj = parse_project(src).unwrap();
        assert_eq!(proj.common.modules[0].version.as_deref(), Some("0.7"));
    }

    #[test]
    fn parse_return_with_status() {
        let src = r#"
service API:
    GET "/hello":
        response 201 string
        call db.hello
"#;
        let proj = parse_project(src).unwrap();
        assert_eq!(proj.services[0].http.routes[0].responses[0].status, 201);
    }

    #[test]
    fn parse_return_status_only() {
        let src = r#"
service API:
    POST "/users":
        response 204
        call db.create
"#;
        let proj = parse_project(src).unwrap();
        assert_eq!(proj.services[0].http.routes[0].responses[0].status, 204);
        assert_eq!(proj.services[0].http.routes[0].responses[0].ty, Type::Void);
    }

    #[test]
    fn parse_enum_and_optional() {
        let src = r#"
enum Status:
    Active
    Disabled

type User:
    name?: string(min=2, max=10)
    status: Status
    meta: { age: int, tags?: array<string> }

service API:
    GET "/users":
        response User
        call db.list
"#;
        let proj = parse_project(src).unwrap();
        let svc = &proj.services[0];
        assert_eq!(svc.schema.enums.len(), 1);
        assert_eq!(svc.schema.types.len(), 1);
    }

    #[test]
    fn parse_type_inline_object() {
        let src = r#"
type Address = {
    street: string
    zip: int
}

service API:
    GET "/addr":
        response Address
        call db.get
"#;
        let proj = parse_project(src).unwrap();
        assert_eq!(proj.services[0].schema.types.len(), 1);
    }

    #[test]
    fn parse_versioned_contracts() {
        let src = r#"
version 1

contract User:
    id: int

contract User@2:
    id: int
    email: string

service API:
    GET "/":
        response User
        call db.hello
"#;
        let proj = parse_project(src).unwrap();
        let svc = &proj.services[0];
        assert_eq!(svc.schema.types.len(), 2);
        let v1 = svc
            .schema
            .types
            .iter()
            .find(|t| t.name.version == Some(1))
            .unwrap();
        let v2 = svc
            .schema
            .types
            .iter()
            .find(|t| t.name.version == Some(2))
            .unwrap();
        assert_eq!(v1.name.name, "User");
        assert_eq!(v2.name.name, "User");
        assert_eq!(
            svc.http.routes[0].responses[0].ty,
            Type::Named(TypeName {
                name: "User".to_string(),
                version: Some(1),
            })
        );
    }

    #[test]
    fn parse_event_payload() {
        let src = r#"
version 2

contract User:
    id: int

event UserCreated:
    payload User

service API:
    emit UserCreated
    subscribe UserCreated
    GET "/":
        response User
        call db.hello
"#;
        let proj = parse_project(src).unwrap();
        let svc = &proj.services[0];
        assert_eq!(svc.events.definitions.len(), 1);
        assert_eq!(svc.events.definitions[0].name.version, Some(2));
        assert_eq!(
            svc.events.definitions[0].payload,
            Type::Named(TypeName {
                name: "User".to_string(),
                version: Some(2)
            })
        );
        assert_eq!(svc.events.emits.len(), 1);
        assert_eq!(svc.events.subscribes.len(), 1);
        assert_eq!(svc.events.emits[0].version, Some(2));
    }

    #[test]
    fn parse_rpc_decl() {
        let src = r#"
version 1

contract User:
    id: int

rpc Auth.Login:
    input:
        username: string
        password: string
    output User
    call auth.login(input.username, input.password)

service Auth:
    GET "/":
        response User
        call db.hello
"#;
        let proj = parse_project(src).unwrap();
        let svc = &proj.services[0];
        assert_eq!(svc.rpc.methods.len(), 1);
        assert_eq!(svc.rpc.methods[0].service, "Auth");
        assert_eq!(svc.rpc.methods[0].version, Some(1));
        assert_eq!(svc.rpc.methods[0].input.len(), 2);
    }

    #[test]
    fn parse_service_config_blocks_and_legacy_directives() {
        let src = r#"
service API rust:
    address "127.0.0.1"
    port 8080
    base_url "https://api.example.com"
    rate_limit 10/min
    env DATABASE_URL="sqlite://dev.db"
    config:
        cors: "https://example.com"
        cors_methods: "GET,POST"
        cors_headers: "authorization,x-request-id"
    grpc_config:
        address: "127.0.0.1"
        port: 50051
    events_config:
        broker: redis
        url: "redis://127.0.0.1:6379"
        group: "events"
    GET "/health":
        response 200
        healthcheck
"#;
        let proj = parse_project(src).unwrap();
        let svc = &proj.services[0];
        assert_eq!(svc.common.address.as_deref(), Some("127.0.0.1"));
        assert_eq!(svc.common.port, Some(8080));
        assert_eq!(
            svc.common.base_url.as_deref(),
            Some("https://api.example.com")
        );
        assert!(svc.common.cors.is_some());
        assert_eq!(svc.rpc.grpc_config.as_ref().unwrap().port, Some(50051));
        assert_eq!(
            svc.events
                .config
                .as_ref()
                .and_then(|cfg| cfg.group.as_deref()),
            Some("events")
        );
        assert_eq!(svc.common.env.len(), 1);
    }

    #[test]
    fn reject_invalid_base_url_and_cors_values() {
        let invalid_base_url = r#"
service API:
    config:
        base_url: "ftp://example.com"
    GET "/health":
        response 200
        healthcheck
"#;
        let err = parse_project(invalid_base_url).unwrap_err().to_string();
        assert!(err.contains("base_url must start with 'http://' or 'https://'"));

        let invalid_cors_method = r#"
service API:
    config:
        cors_methods: "GET,PURGE"
    GET "/health":
        response 200
        healthcheck
"#;
        let err = parse_project(invalid_cors_method).unwrap_err().to_string();
        assert!(err.contains("invalid cors method 'PURGE'"));

        let invalid_cors_header = r#"
service API:
    config:
        cors_headers: "authorization,bad header"
    GET "/health":
        response 200
        healthcheck
"#;
        let err = parse_project(invalid_cors_header).unwrap_err().to_string();
        assert!(err.contains("invalid cors header 'bad header'"));
    }
}
