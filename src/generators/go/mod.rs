use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::generators::rpc;
use crate::ir::Service;

pub(crate) mod events;
mod main;
mod middleware;
mod remote_calls;
mod sockets;
mod types;
pub(crate) mod util;
mod validation;
mod wrappers;

pub fn generate_go_server(service: &Service, out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("Failed to create '{}'", out_dir.display()))?;

    let module_name = format!("nimesvc/{}-api", service.name.to_lowercase());
    let go_mod = util::build_go_mod(service, &module_name);
    fs::write(out_dir.join("go.mod"), go_mod).with_context(|| "Failed to write go.mod")?;

    let types_dir = out_dir.join("types");
    fs::create_dir_all(&types_dir)
        .with_context(|| format!("Failed to create '{}'", types_dir.display()))?;
    let types_go = types::build_types_go(service);
    fs::write(types_dir.join("types.go"), types_go)
        .with_context(|| "Failed to write types/types.go")?;

    let main_go = main::build_main_go(service, &module_name);
    fs::write(out_dir.join("main.go"), main_go).with_context(|| "Failed to write main.go")?;

    if !service.events.definitions.is_empty() {
        let events_go = events::build_events_go(service, &module_name);
        fs::write(out_dir.join("events.go"), events_go)
            .with_context(|| "Failed to write events.go")?;
    }

    let rpc_proto = rpc::build_proto(service);
    if !rpc_proto.is_empty() {
        fs::write(out_dir.join("rpc.proto"), rpc_proto)
            .with_context(|| "Failed to write rpc.proto")?;
    }

    if util::needs_remote_calls(service) {
        let remote_calls_go = remote_calls::build_remote_calls_go(service, &module_name);
        fs::write(out_dir.join("remote_calls.go"), remote_calls_go)
            .with_context(|| "Failed to write remote_calls.go")?;
    }

    let middleware_go = middleware::build_middleware_go(service);
    if !middleware_go.is_empty() {
        fs::write(out_dir.join("middleware.go"), middleware_go)
            .with_context(|| "Failed to write middleware.go")?;
    }

    wrappers::write_go_module_wrappers(service, out_dir)?;

    Ok(())
}
