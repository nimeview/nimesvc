use std::fs;
use std::path::PathBuf;

use nimesvc::generators::go::generate_go_server;
use nimesvc::generators::rust::generate_rust_server;
use nimesvc::generators::typescript::generate_ts_server;
use nimesvc::parser::parse_project;

fn temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("nimesvc_events_{}_{}", prefix, stamp));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn memory_events_service() -> nimesvc::ir::Service {
    let src = r#"
event UserCreated:
    payload { id: string, active: bool }

service Events rust:
    emit UserCreated
    GET "/health":
        response 200
        healthcheck
"#;
    parse_project(src)
        .unwrap()
        .services
        .into_iter()
        .next()
        .unwrap()
}

fn redis_events_service() -> nimesvc::ir::Service {
    let src = r#"
event UserCreated:
    payload { id: string, active: bool }

service Events rust:
    events_config:
        broker: redis
        url: "redis://127.0.0.1:6379"
        group: "events"
        consumer: "events-1"
        stream_prefix: "app"
    emit UserCreated
    subscribe UserCreated
    GET "/health":
        response 200
        healthcheck
"#;
    parse_project(src)
        .unwrap()
        .services
        .into_iter()
        .next()
        .unwrap()
}

#[test]
fn generates_memory_event_helpers() {
    let service = memory_events_service();

    let rust_dir = temp_dir("memory_rust");
    generate_rust_server(&service, &rust_dir).unwrap();
    let rust_events = fs::read_to_string(rust_dir.join("src").join("events.rs")).unwrap();
    let rust_main = fs::read_to_string(rust_dir.join("src").join("main.rs")).unwrap();
    assert!(rust_events.contains("pub fn on_user_created"));
    assert!(rust_events.contains("pub fn emit_user_created"));
    assert!(rust_events.contains("Arc<dyn Fn("));
    assert!(!rust_main.contains("start_event_consumers().await"));

    let ts_dir = temp_dir("memory_ts");
    generate_ts_server(&service, &ts_dir).unwrap();
    let ts_events = fs::read_to_string(ts_dir.join("src").join("events.ts")).unwrap();
    let ts_main = fs::read_to_string(ts_dir.join("src").join("index.ts")).unwrap();
    assert!(ts_events.contains("export function onUserCreated"));
    assert!(ts_events.contains("export function emitUserCreated"));
    assert!(!ts_main.contains("startEventConsumers();"));

    let go_dir = temp_dir("memory_go");
    generate_go_server(&service, &go_dir).unwrap();
    let go_events = fs::read_to_string(go_dir.join("events.go")).unwrap();
    let go_main = fs::read_to_string(go_dir.join("main.go")).unwrap();
    assert!(go_events.contains("func OnUserCreated"));
    assert!(go_events.contains("func EmitUserCreated"));
    assert!(!go_main.contains("StartEventConsumers()"));
}

#[test]
fn generates_redis_event_helpers_and_starts_consumers() {
    let service = redis_events_service();

    let rust_dir = temp_dir("redis_rust");
    generate_rust_server(&service, &rust_dir).unwrap();
    let rust_events = fs::read_to_string(rust_dir.join("src").join("events.rs")).unwrap();
    let rust_main = fs::read_to_string(rust_dir.join("src").join("main.rs")).unwrap();
    let rust_cargo = fs::read_to_string(rust_dir.join("Cargo.toml")).unwrap();
    assert!(rust_events.contains("EVENTS_STREAM_PREFIX"));
    assert!(rust_events.contains("publish_event"));
    assert!(rust_events.contains("start_event_consumers"));
    assert!(rust_main.contains("events::start_event_consumers().await;"));
    assert!(rust_cargo.contains("redis ="));

    let ts_dir = temp_dir("redis_ts");
    generate_ts_server(&service, &ts_dir).unwrap();
    let ts_events = fs::read_to_string(ts_dir.join("src").join("events.ts")).unwrap();
    let ts_main = fs::read_to_string(ts_dir.join("src").join("index.ts")).unwrap();
    let ts_pkg = fs::read_to_string(ts_dir.join("package.json")).unwrap();
    assert!(ts_events.contains("createClient"));
    assert!(ts_events.contains("startEventConsumers"));
    assert!(ts_main.contains("events.startEventConsumers();"));
    assert!(ts_pkg.contains("\"redis\":"));

    let go_dir = temp_dir("redis_go");
    generate_go_server(&service, &go_dir).unwrap();
    let go_events = fs::read_to_string(go_dir.join("events.go")).unwrap();
    let go_main = fs::read_to_string(go_dir.join("main.go")).unwrap();
    let go_mod = fs::read_to_string(go_dir.join("go.mod")).unwrap();
    assert!(go_events.contains("func StartEventConsumers()"));
    assert!(go_events.contains("XReadGroup"));
    assert!(go_main.contains("StartEventConsumers()"));
    assert!(go_mod.contains("github.com/redis/go-redis/v9"));
}
