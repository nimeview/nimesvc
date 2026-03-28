use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::generators::rpc;
use crate::ir::Service;

pub(crate) mod events;
mod main;
mod middleware;
mod remote_calls;
mod routes;
mod sockets;
mod types;
mod util;
mod validation;

pub fn generate_rust_server(service: &Service, out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("Failed to create '{}'", out_dir.display()))?;
    let src_dir = out_dir.join("src");
    fs::create_dir_all(&src_dir)
        .with_context(|| format!("Failed to create '{}'", src_dir.display()))?;
    let crate_name = format!("{}-api", util::to_kebab_case(&service.name));

    let cargo_toml = util::build_cargo_toml(service, &crate_name);
    let main_rs = main::build_main_rs(service);
    let types_rs = types::build_types_rs(service);
    let events_rs = if service.events.definitions.is_empty() {
        None
    } else {
        Some(events::build_events_rs(service))
    };
    let remote_calls_rs = if util::needs_remote_calls(service) {
        Some(remote_calls::build_remote_calls_rs(service))
    } else {
        None
    };
    let rpc_proto = rpc::build_proto(service);

    fs::write(out_dir.join("Cargo.toml"), cargo_toml)
        .with_context(|| "Failed to write Cargo.toml")?;
    fs::write(src_dir.join("main.rs"), main_rs).with_context(|| "Failed to write src/main.rs")?;
    fs::write(src_dir.join("types.rs"), types_rs)
        .with_context(|| "Failed to write src/types.rs")?;
    if !rpc_proto.is_empty() {
        fs::write(out_dir.join("rpc.proto"), rpc_proto)
            .with_context(|| "Failed to write rpc.proto")?;
    }
    if let Some(events_rs) = events_rs {
        fs::write(src_dir.join("events.rs"), events_rs)
            .with_context(|| "Failed to write src/events.rs")?;
    }
    if let Some(remote_calls_rs) = remote_calls_rs {
        fs::write(src_dir.join("remote_calls.rs"), remote_calls_rs)
            .with_context(|| "Failed to write src/remote_calls.rs")?;
    }
    middleware::generate_middleware(service, &src_dir)?;

    Ok(())
}
