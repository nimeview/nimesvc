use std::fs;
use std::path::PathBuf;

use nimesvc::generators::go::generate_go_server;
use nimesvc::generators::grpc::generate_grpc_server;
use nimesvc::generators::rust::generate_rust_server;
use nimesvc::generators::typescript::generate_ts_server;
use nimesvc::ir::Lang;
use nimesvc::parser::parse_project;

fn temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("nimesvc_scaffold_{}_{}", prefix, stamp));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn scaffold_service() -> nimesvc::ir::Service {
    let src = r#"
rpc API.Login:
    output string
    middleware rpc_audit
    call auth.login

service API:
    auth bearer
    middleware trace

    GET "/health":
        response 200
        middleware: audit
        healthcheck

    socket Chat "/ws":
        middleware socket_guard
        inbound:
            Join -> chat.onJoin
        outbound:
            MessageOut -> chat.onMessage
"#;
    parse_project(src)
        .unwrap()
        .services
        .into_iter()
        .next()
        .unwrap()
}

#[test]
fn generated_servers_and_grpc_do_not_emit_todo_scaffolding() {
    let service = scaffold_service();

    let rust_dir = temp_dir("rust");
    generate_rust_server(&service, &rust_dir).unwrap();
    let rust_main = fs::read_to_string(rust_dir.join("src").join("main.rs")).unwrap();
    let rust_middleware = fs::read_to_string(rust_dir.join("src").join("middleware.rs")).unwrap();
    assert!(!rust_main.contains("TODO:"));
    assert!(!rust_middleware.contains("TODO:"));
    assert!(rust_middleware.contains("auth_bearer"));
    assert!(rust_main.contains("empty socket message kind"));

    let ts_dir = temp_dir("ts");
    generate_ts_server(&service, &ts_dir).unwrap();
    let ts_index = fs::read_to_string(ts_dir.join("src").join("index.ts")).unwrap();
    let ts_middleware = fs::read_to_string(ts_dir.join("src").join("middleware.ts")).unwrap();
    assert!(!ts_index.contains("TODO:"));
    assert!(!ts_middleware.contains("TODO:"));
    assert!(ts_middleware.contains("auth_bearer"));
    assert!(ts_index.contains("empty socket message kind"));

    let go_dir = temp_dir("go");
    generate_go_server(&service, &go_dir).unwrap();
    let go_main = fs::read_to_string(go_dir.join("main.go")).unwrap();
    let go_middleware = fs::read_to_string(go_dir.join("middleware.go")).unwrap();
    assert!(!go_main.contains("TODO:"));
    assert!(!go_middleware.contains("TODO:"));
    assert!(go_middleware.contains("auth_bearer"));
    assert!(go_main.contains("empty socket message kind"));

    let grpc_rust_dir = temp_dir("grpc_rust");
    generate_grpc_server(&service, &grpc_rust_dir, Lang::Rust).unwrap();
    let grpc_rust_main = fs::read_to_string(grpc_rust_dir.join("src").join("main.rs")).unwrap();
    let grpc_rust_middleware =
        fs::read_to_string(grpc_rust_dir.join("src").join("middleware.rs")).unwrap();
    assert!(!grpc_rust_main.contains("TODO:"));
    assert!(!grpc_rust_middleware.contains("TODO:"));
    assert!(grpc_rust_main.contains("missing authorization"));
    assert!(grpc_rust_middleware.contains("blocked by middleware rpc_audit"));

    let grpc_ts_dir = temp_dir("grpc_ts");
    generate_grpc_server(&service, &grpc_ts_dir, Lang::TypeScript).unwrap();
    let grpc_ts_index = fs::read_to_string(grpc_ts_dir.join("src").join("index.ts")).unwrap();
    assert!(!grpc_ts_index.contains("TODO:"));
    assert!(grpc_ts_index.contains("missing authorization"));
    assert!(grpc_ts_index.contains("blocked by middleware rpc_audit"));

    let grpc_go_dir = temp_dir("grpc_go");
    generate_grpc_server(&service, &grpc_go_dir, Lang::Go).unwrap();
    let grpc_go_server = fs::read_to_string(grpc_go_dir.join("server.go")).unwrap();
    let grpc_go_middleware = fs::read_to_string(grpc_go_dir.join("middleware.go")).unwrap();
    assert!(!grpc_go_server.contains("TODO:"));
    assert!(!grpc_go_middleware.contains("TODO:"));
    assert!(grpc_go_server.contains("missing authorization"));
    assert!(grpc_go_middleware.contains("blocked by middleware rpc_audit"));
}

#[test]
fn rust_codegen_avoids_panic_style_serialization_and_cors_parsing() {
    let src = r#"
service API rust:
    config:
        cors: "https://example.com,https://api.example.com"
        cors_methods: "GET,POST"
        cors_headers: "authorization,x-request-id"

    POST "/users":
        input:
            body:
                name: string
        responses:
            200 string
        call users.create(body.name)
"#;
    let service = parse_project(src)
        .unwrap()
        .services
        .into_iter()
        .next()
        .unwrap();

    let rust_server_dir = temp_dir("rust_hardening_server");
    generate_rust_server(&service, &rust_server_dir).unwrap();
    let rust_main = fs::read_to_string(rust_server_dir.join("src").join("main.rs")).unwrap();
    assert!(rust_main.contains("fn cors_methods(values: &[&str])"));
    assert!(rust_main.contains("AllowOrigin::list(cors_origins(&["));
    assert!(!rust_main.contains("HeaderValue::from_str(\"https://example.com\").unwrap()"));
    assert!(!rust_main.contains("Method::from_bytes(b\"GET\").unwrap()"));
}
