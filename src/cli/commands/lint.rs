use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use nimesvc::parser::parse_project;

pub(super) fn lint_cmd(input: PathBuf) -> Result<()> {
    let src = fs::read_to_string(&input)
        .with_context(|| format!("Failed to read input file '{}'", input.display()))?;
    let project = parse_project(&src)?;

    let mut warnings = Vec::new();

    if project.services.is_empty() {
        warnings.push("No services declared".to_string());
    }

    let mut global_used = std::collections::HashSet::new();
    for service in &project.services {
        let mut used = std::collections::HashSet::new();
        for route in &service.http.routes {
            if route.call.service_base.is_none() {
                used.insert(route.call.module.clone());
            }
        }
        for rpc in &service.rpc.methods {
            used.insert(rpc.call.module.clone());
        }
        for socket in &service.sockets.sockets {
            for msg in socket.inbound.iter().chain(socket.outbound.iter()) {
                used.insert(msg.handler.module.clone());
            }
        }

        if service.http.routes.is_empty()
            && service.rpc.methods.is_empty()
            && service.sockets.sockets.is_empty()
        {
            warnings.push(format!(
                "Service '{}' has no routes, rpcs, or sockets",
                service.name
            ));
        }

        if matches!(
            service.events.config.as_ref().map(|c| &c.broker),
            Some(nimesvc::ir::EventsBroker::Redis)
        ) && service.events.emits.is_empty()
            && service.events.subscribes.is_empty()
        {
            warnings.push(format!(
                "Service '{}' has events_config but no emit/subscribe",
                service.name
            ));
        }

        for module in &service.common.modules {
            let alias = module.alias.as_deref().unwrap_or(&module.name);
            if !used.contains(alias) {
                warnings.push(format!(
                    "Service '{}' module '{}' is not used",
                    service.name, alias
                ));
            }
        }

        for module in &project.common.modules {
            let alias = module.alias.as_deref().unwrap_or(&module.name);
            if used.contains(alias) {
                global_used.insert(alias.to_string());
            }
        }
    }

    for module in &project.common.modules {
        let alias = module.alias.as_deref().unwrap_or(&module.name);
        if !global_used.contains(alias) {
            warnings.push(format!("Global module '{}' is not used", alias));
        }
    }

    if warnings.is_empty() {
        println!("Lint: OK");
        return Ok(());
    }

    println!("Lint: warnings");
    for warning in &warnings {
        println!(" - {}", warning);
    }
    Ok(())
}
