use std::fs;
use std::path::PathBuf;
use std::process::Command;

use nimesvc::generators::grpc::generate_grpc_server;
use nimesvc::ir::Lang;
use nimesvc::parser::parse_project;

fn temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("nimesvc_grpc_{}_{}", prefix, stamp));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn grpc_service() -> nimesvc::ir::Service {
    let src = r#"
use auth "./modules/auth.rs"

rpc Auth.Login:
    input:
        username: string
        password: string
    headers:
        x_request_id?: string
    output string
    auth bearer
    middleware rpc_audit
    rate_limit 10/min
    call auth.login(input.username, input.password)

service Auth:
    auth bearer
    middleware trace
    grpc_config:
        address: "127.0.0.1"
        port: 50061
        max_message_size: 4mb
        tls "./cert.pem" "./key.pem"
"#;
    parse_project(src)
        .unwrap()
        .services
        .into_iter()
        .next()
        .unwrap()
}

#[test]
fn grpc_generators_apply_config_and_emit_expected_layout() {
    let service = grpc_service();

    let rust_dir = temp_dir("rust");
    generate_grpc_server(&service, &rust_dir, Lang::Rust).unwrap();
    let rust_main = fs::read_to_string(rust_dir.join("src").join("main.rs")).unwrap();
    let rust_build = fs::read_to_string(rust_dir.join("build.rs")).unwrap();
    let rust_proto = fs::read_to_string(rust_dir.join("rpc.proto")).unwrap();
    assert!(rust_dir.join("Cargo.toml").exists());
    assert!(rust_main.contains("let addr = \"127.0.0.1:50061\".parse()?;"));
    assert!(rust_main.contains("max_decoding_message_size(4194304 as usize)"));
    assert!(rust_main.contains("Identity::from_pem(cert, key)"));
    assert!(rust_main.contains("builder = builder.tls_config(tls)?;"));
    assert!(rust_build.contains("tonic_build::configure()"));
    assert!(rust_proto.contains("service Auth"));

    let go_dir = temp_dir("go");
    generate_grpc_server(&service, &go_dir, Lang::Go).unwrap();
    let go_main = fs::read_to_string(go_dir.join("main.go")).unwrap();
    let go_server = fs::read_to_string(go_dir.join("server.go")).unwrap();
    let go_gen = fs::read_to_string(go_dir.join("gen.sh")).unwrap();
    let go_proto = fs::read_to_string(go_dir.join("proto").join("rpc.proto")).unwrap();
    assert!(go_dir.join("go.mod").exists());
    assert!(go_dir.join("types").join("types.go").exists());
    assert!(go_main.contains("net.Listen(\"tcp\", \"127.0.0.1:50061\")"));
    assert!(go_main.contains("grpc.MaxRecvMsgSize(int(4194304))"));
    assert!(go_main.contains("credentials.NewServerTLSFromFile(\"./cert.pem\", \"./key.pem\")"));
    assert!(go_server.contains("missing authorization"));
    assert!(go_gen.contains("protoc"));
    assert!(go_proto.contains("service Auth"));

    let ts_dir = temp_dir("ts");
    generate_grpc_server(&service, &ts_dir, Lang::TypeScript).unwrap();
    let ts_index = fs::read_to_string(ts_dir.join("src").join("index.ts")).unwrap();
    let ts_pkg = fs::read_to_string(ts_dir.join("package.json")).unwrap();
    let ts_proto = fs::read_to_string(ts_dir.join("rpc.proto")).unwrap();
    assert!(ts_dir.join("tsconfig.json").exists());
    assert!(ts_index.contains("server.bindAsync('127.0.0.1:50061'"));
    assert!(ts_index.contains("serverOptions['grpc.max_receive_message_length'] = 4194304;"));
    assert!(ts_index.contains("grpc.ServerCredentials.createSsl"));
    assert!(ts_index.contains("missing authorization"));
    assert!(ts_pkg.contains("@grpc/grpc-js"));
    assert!(ts_proto.contains("service Auth"));
}

#[test]
fn grpc_generators_use_default_listener_when_grpc_config_is_absent() {
    let src = r#"
use auth "./modules/auth.rs"

rpc Auth.Login:
    output string
    call auth.login

service Auth:
"#;
    let service = parse_project(src)
        .unwrap()
        .services
        .into_iter()
        .next()
        .unwrap();

    let rust_dir = temp_dir("rust_default");
    generate_grpc_server(&service, &rust_dir, Lang::Rust).unwrap();
    let rust_main = fs::read_to_string(rust_dir.join("src").join("main.rs")).unwrap();
    assert!(rust_main.contains("let addr = \"127.0.0.1:50051\".parse()?;"));

    let go_dir = temp_dir("go_default");
    generate_grpc_server(&service, &go_dir, Lang::Go).unwrap();
    let go_main = fs::read_to_string(go_dir.join("main.go")).unwrap();
    assert!(go_main.contains("net.Listen(\"tcp\", \"127.0.0.1:50051\")"));

    let ts_dir = temp_dir("ts_default");
    generate_grpc_server(&service, &ts_dir, Lang::TypeScript).unwrap();
    let ts_index = fs::read_to_string(ts_dir.join("src").join("index.ts")).unwrap();
    assert!(ts_index.contains("server.bindAsync('127.0.0.1:50051'"));
}

#[test]
fn generate_without_grpc_kind_also_emits_grpc_output() {
    let bin = env!("CARGO_BIN_EXE_nimesvc");
    let base = temp_dir("auto_generate");
    let modules_dir = base.join("modules");
    fs::create_dir_all(&modules_dir).unwrap();
    let ns_path = base.join("api.ns");
    let out_dir = base.join("build");

    let ns = r#"
use auth "./modules/auth.rs"

rpc Auth.Login:
    input:
        username: string
    output string
    call auth.login(input.username)

service Auth rust:
    grpc_config:
        address: "127.0.0.1"
        port: 50061

    GET "/health":
        response 200
        healthcheck
"#;
    fs::write(&ns_path, ns).unwrap();
    fs::write(
        modules_dir.join("auth.rs"),
        "pub fn login(username: String) -> String { username }\n",
    )
    .unwrap();

    let status = Command::new(bin)
        .arg("generate")
        .arg(&ns_path)
        .arg("--out")
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(status.success());

    assert!(out_dir.join("Auth").join("Cargo.toml").exists());
    assert!(out_dir.join("Auth-grpc").join("Cargo.toml").exists());
    assert!(out_dir.join("Auth-grpc").join("rpc.proto").exists());
}
