use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::generators::rpc;
use crate::ir::Service;

pub(crate) mod events;
mod main;
mod middleware;
mod package;
mod remote_calls;
mod routes;
mod sockets;
pub(crate) mod types;
pub(crate) mod util;

pub fn generate_ts_server(service: &Service, out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("Failed to create '{}'", out_dir.display()))?;
    let src_dir = out_dir.join("src");
    fs::create_dir_all(&src_dir)
        .with_context(|| format!("Failed to create '{}'", src_dir.display()))?;
    let main_ts = main::build_main_ts(service);
    fs::write(src_dir.join("index.ts"), main_ts).with_context(|| "Failed to write src/index.ts")?;

    let types_ts = types::build_types_ts(service);
    fs::write(src_dir.join("types.ts"), types_ts)
        .with_context(|| "Failed to write src/types.ts")?;
    if !service.events.definitions.is_empty() {
        let events_ts = events::build_events_ts(service);
        fs::write(src_dir.join("events.ts"), events_ts)
            .with_context(|| "Failed to write src/events.ts")?;
    }
    if util::needs_remote_calls(service) {
        let remote_calls_ts = remote_calls::build_remote_calls_ts(service);
        fs::write(src_dir.join("remote_calls.ts"), remote_calls_ts)
            .with_context(|| "Failed to write src/remote_calls.ts")?;
    }

    let rpc_proto = rpc::build_proto(service);
    if !rpc_proto.is_empty() {
        fs::write(out_dir.join("rpc.proto"), rpc_proto)
            .with_context(|| "Failed to write rpc.proto")?;
    }

    let package_json = package::build_package_json(service);
    fs::write(out_dir.join("package.json"), package_json)
        .with_context(|| "Failed to write package.json")?;

    let tsconfig = r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "CommonJS",
    "outDir": "dist",
    "esModuleInterop": true,
    "strict": true,
    "lib": ["ES2020", "DOM"]
  }
}
"#;
    fs::write(out_dir.join("tsconfig.json"), tsconfig)
        .with_context(|| "Failed to write tsconfig.json")?;

    let install = "npm install\n";
    fs::write(out_dir.join("install.sh"), install).with_context(|| "Failed to write install.sh")?;

    middleware::generate_middleware(service, &src_dir)?;

    Ok(())
}
