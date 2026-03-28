use std::fs;
use std::path::PathBuf;

use nimesvc::ir::{HttpMethod, Lang, Type, TypeName};
use nimesvc::parser::parse_project;

fn load_auth_data() -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("examples/auth_data.ns");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

fn find_route<'a>(
    svc: &'a nimesvc::ir::Service,
    method: HttpMethod,
    path: &str,
) -> &'a nimesvc::ir::Route {
    svc.http
        .routes
        .iter()
        .find(|r| r.method == method && r.path == path)
        .unwrap_or_else(|| panic!("route {method:?} {path} not found"))
}

#[test]
fn parse_auth_data_project() {
    let src = load_auth_data();
    let proj = parse_project(&src).expect("parse_project failed");

    assert_eq!(proj.common.output.as_deref(), Some("./.nimesvc"));
    assert_eq!(proj.services.len(), 2);

    let auth = proj
        .services
        .iter()
        .find(|s| s.name == "Auth")
        .expect("Auth service not found");
    let data = proj
        .services
        .iter()
        .find(|s| s.name == "Data")
        .expect("Data service not found");

    assert_eq!(auth.language, Some(Lang::Rust));
    assert_eq!(data.language, Some(Lang::Go));
    assert_eq!(
        auth.common.base_url.as_deref(),
        Some("http://127.0.0.1:8081")
    );
    assert_eq!(
        data.common.base_url.as_deref(),
        Some("http://127.0.0.1:8082")
    );

    let login = find_route(auth, HttpMethod::Post, "/login");
    assert_eq!(login.input.body.len(), 2);
    assert_eq!(
        login.responses[0].ty,
        Type::Named(TypeName {
            name: "Token".to_string(),
            version: None
        })
    );

    let validate = find_route(auth, HttpMethod::Get, "/validate/{token}");
    assert_eq!(validate.input.path.len(), 1);
    assert_eq!(validate.responses[0].ty, Type::Bool);

    let audit = find_route(auth, HttpMethod::Post, "/audit");
    assert_eq!(audit.responses[0].status, 201);
    assert_eq!(
        audit.responses[0].ty,
        Type::Named(TypeName {
            name: "Audit".to_string(),
            version: None
        })
    );
    assert_eq!(audit.call.service.as_deref(), Some("Data"));
    assert_eq!(audit.call.module, "data");
    assert_eq!(audit.call.function, "add_audit");

    let list_records = find_route(data, HttpMethod::Get, "/records");
    assert_eq!(
        list_records.responses[0].ty,
        Type::Array(Box::new(Type::Named(TypeName {
            name: "Record".to_string(),
            version: None
        })))
    );

    let check = find_route(data, HttpMethod::Get, "/check/{token}");
    assert_eq!(check.call.service.as_deref(), Some("Auth"));
    assert_eq!(check.call.module, "auth");
    assert_eq!(check.call.function, "validate");
}
