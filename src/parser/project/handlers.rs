use anyhow::{Result, anyhow, bail};

use super::state::ProjectState;

use super::super::events::{finalize_event, parse_event_decl, parse_event_payload_line};
use super::super::model::InputMode;
use super::super::route::{
    finalize_route, parse_input_field_line, parse_input_section_line, parse_route_body_line,
    parse_route_decl,
};
use super::super::rpc::{finalize_rpc, parse_rpc_body_line, parse_rpc_decl, parse_rpc_input_field};
use super::super::service::{
    ServiceBlock, ServiceBuilder, finalize_service, parse_events_config_line,
    parse_grpc_config_line, parse_header_field_line, parse_response_line,
    parse_service_config_line, parse_service_decl, validate_base_url,
};
use super::super::socket::{
    finalize_socket, finalize_trigger, parse_socket_body_line, parse_socket_decl,
    parse_socket_message_line, parse_trigger_body_line, parse_trigger_decl,
};
use super::super::types::{
    finalize_enum, finalize_type, parse_enum_decl, parse_enum_variant_line, parse_type_decl,
    parse_type_field_line,
};
use super::super::use_decl::{parse_auth_decl, parse_middleware_decl, parse_use_decl};
use super::super::util::parse_quoted_value;

pub(super) fn handle_indent0(
    state: &mut ProjectState,
    content: &str,
    line_no: usize,
) -> Result<()> {
    if content.starts_with("service ") {
        finalize_global_decls(state, line_no)?;
        finalize_current_service(state, line_no)?;
        let (name, lang) = parse_service_decl(content, line_no)?;
        state.current_service = Some(ServiceBuilder {
            name,
            language: lang,
            modules: Vec::new(),
            auth: None,
            middleware: Vec::new(),
            address: None,
            port: None,
            grpc_config: None,
            events_config: None,
            base_url: None,
            cors: None,
            rate_limit: None,
            env: Vec::new(),
            headers: Vec::new(),
            types: Vec::new(),
            enums: Vec::new(),
            events: Vec::new(),
            rpcs: Vec::new(),
            sockets: Vec::new(),
            emits: Vec::new(),
            subscribes: Vec::new(),
            routes: Vec::new(),
            block: ServiceBlock::None,
        });
        return Ok(());
    }
    if content.starts_with("output ") {
        if state.current_service.is_some() {
            bail!("Line {}: output must appear before service", line_no);
        }
        if state.output.is_some() {
            bail!("Line {}: duplicate output directive", line_no);
        }
        let path = parse_quoted_value(content.strip_prefix("output ").unwrap(), line_no)?;
        state.output = Some(path);
        return Ok(());
    }
    if content.starts_with("version ") {
        if state.current_service.is_some() {
            bail!("Line {}: version must appear before service", line_no);
        }
        if state.default_version.is_some() {
            bail!("Line {}: duplicate version directive", line_no);
        }
        let raw = content.strip_prefix("version ").unwrap().trim();
        let parsed: u32 = raw
            .parse()
            .map_err(|_| anyhow!("Line {}: invalid version '{}'", line_no, raw))?;
        state.default_version = Some(parsed);
        return Ok(());
    }
    if content.starts_with("use ") {
        if state.current_service.is_some() {
            bail!(
                "Line {}: use must appear before service or inside it",
                line_no
            );
        }
        let module = parse_use_decl(content, line_no)?;
        state.modules.push(module);
        return Ok(());
    }
    if content.starts_with("auth ") {
        if state.current_service.is_some() {
            bail!(
                "Line {}: auth must appear before service or inside it",
                line_no
            );
        }
        if state.auth.is_some() {
            bail!("Line {}: duplicate auth directive", line_no);
        }
        state.auth = Some(parse_auth_decl(content, line_no)?);
        return Ok(());
    }
    if content.starts_with("middleware ") {
        if state.current_service.is_some() {
            bail!(
                "Line {}: middleware must appear before service or inside it",
                line_no
            );
        }
        let mw = parse_middleware_decl(content, line_no)?;
        state.middleware.push(mw);
        return Ok(());
    }
    if content.starts_with("type ") || content.starts_with("contract ") {
        if state.current_service.is_some() {
            bail!(
                "Line {}: type declarations must appear before service",
                line_no
            );
        }
        finalize_global_decls(state, line_no)?;
        if state.current_socket.is_some() {
            bail!("Line {}: socket must be inside service", line_no);
        }
        let tb = parse_type_decl(content, line_no)?;
        state.current_type = Some(tb);
        return Ok(());
    }
    if content.starts_with("enum ") {
        if state.current_service.is_some() {
            bail!(
                "Line {}: enum declarations must appear before service",
                line_no
            );
        }
        finalize_global_decls(state, line_no)?;
        if state.current_socket.is_some() {
            bail!("Line {}: socket must be inside service", line_no);
        }
        let eb = parse_enum_decl(content, line_no)?;
        state.current_enum = Some(eb);
        return Ok(());
    }
    if content.starts_with("event ") {
        if state.current_service.is_some() {
            bail!(
                "Line {}: event declarations must appear before service",
                line_no
            );
        }
        finalize_global_decls(state, line_no)?;
        if state.current_socket.is_some() {
            bail!("Line {}: socket must be inside service", line_no);
        }
        let eb = parse_event_decl(content, line_no)?;
        state.current_event = Some(eb);
        return Ok(());
    }
    if content.starts_with("rpc ") {
        if state.current_service.is_some() {
            bail!(
                "Line {}: rpc declarations must appear before service",
                line_no
            );
        }
        finalize_global_decls(state, line_no)?;
        if state.current_socket.is_some() {
            bail!("Line {}: socket must be inside service", line_no);
        }
        let rb = parse_rpc_decl(content, line_no)?;
        state.current_rpc = Some(rb);
        return Ok(());
    }

    bail!(
        "Line {}: expected `service`, `type`, `contract`, `event`, or `rpc` declaration",
        line_no
    );
}

pub(super) fn handle_indent4(
    state: &mut ProjectState,
    content: &str,
    line_no: usize,
) -> Result<()> {
    if state.current_type.is_some() {
        if state.current_type.as_ref().unwrap().sealed {
            bail!(
                "Line {}: type '{}' is sealed and cannot have block fields",
                line_no,
                state.current_type.as_ref().unwrap().name.display()
            );
        }
        parse_type_field_line(state.current_type.as_mut().unwrap(), content, line_no)?;
        return Ok(());
    }
    if state.current_enum.is_some() {
        parse_enum_variant_line(state.current_enum.as_mut().unwrap(), content, line_no)?;
        return Ok(());
    }
    if state.current_event.is_some() {
        parse_event_payload_line(state.current_event.as_mut().unwrap(), content, line_no)?;
        return Ok(());
    }
    if state.current_rpc.is_some() {
        parse_rpc_body_line(state.current_rpc.as_mut().unwrap(), content, line_no)?;
        return Ok(());
    }
    let Some(service) = state.current_service.as_mut() else {
        bail!("Line {}: route must be inside service", line_no);
    };

    if let Some(trigger) = finalize_trigger(state.current_trigger.take(), line_no)? {
        if let Some(sock) = state.current_socket.as_mut() {
            sock.triggers.push(trigger);
        }
    }
    if state.current_socket.is_some() {
        if let Some(socket) = finalize_socket(state.current_socket.take(), line_no)? {
            service.sockets.push(socket);
        }
    }

    if content.starts_with("use ") {
        let module = parse_use_decl(content, line_no)?;
        service.modules.push(module);
        return Ok(());
    }
    if content.starts_with("emit ") {
        let name = content.strip_prefix("emit ").unwrap().trim();
        let ev = super::super::types::parse_name_with_version(name, line_no)?;
        service.emits.push(ev);
        return Ok(());
    }
    if content.starts_with("subscribe ") {
        let name = content.strip_prefix("subscribe ").unwrap().trim();
        let ev = super::super::types::parse_name_with_version(name, line_no)?;
        service.subscribes.push(ev);
        return Ok(());
    }
    if content == "config:" {
        service.block = ServiceBlock::Config;
        return Ok(());
    }
    if content == "grpc_config:" {
        service.block = ServiceBlock::GrpcConfig;
        return Ok(());
    }
    if content == "events_config:" {
        service.block = ServiceBlock::EventsConfig;
        return Ok(());
    }
    if content == "headers:" {
        service.block = ServiceBlock::Headers;
        return Ok(());
    }
    if content.starts_with("address ") {
        if service.address.is_some() {
            bail!("Line {}: duplicate address directive", line_no);
        }
        let raw = content.strip_prefix("address ").unwrap();
        let addr = parse_quoted_value(raw.trim(), line_no)?;
        service.address = Some(addr);
        return Ok(());
    }
    if content.starts_with("base_url ") {
        if service.base_url.is_some() {
            bail!("Line {}: duplicate base_url directive", line_no);
        }
        let raw = content.strip_prefix("base_url ").unwrap();
        let url = validate_base_url(&parse_quoted_value(raw.trim(), line_no)?, line_no)?;
        service.base_url = Some(url);
        return Ok(());
    }
    if content.starts_with("port ") {
        if service.port.is_some() {
            bail!("Line {}: duplicate port directive", line_no);
        }
        let raw = content.strip_prefix("port ").unwrap().trim();
        let port: u16 = raw
            .parse()
            .map_err(|_| anyhow!("Line {}: invalid port '{}'", line_no, raw))?;
        if port == 0 {
            bail!("Line {}: port must be between 1 and 65535", line_no);
        }
        service.port = Some(port);
        return Ok(());
    }
    if content.starts_with("rate_limit ") {
        if service.rate_limit.is_some() {
            bail!("Line {}: duplicate rate_limit", line_no);
        }
        let raw = content.strip_prefix("rate_limit ").unwrap().trim();
        service.rate_limit = Some(super::super::route::parse_rate_limit(raw, line_no)?);
        return Ok(());
    }
    if content.starts_with("env ") {
        let raw = content.strip_prefix("env ").unwrap().trim();
        let (name, default) = if let Some(eq) = raw.find('=') {
            let name = raw[..eq].trim();
            let value = raw[eq + 1..].trim();
            if !super::super::util::is_ident(name) {
                bail!("Line {}: invalid env name '{}'", line_no, name);
            }
            let default = parse_quoted_value(value, line_no)?;
            (name, Some(default))
        } else {
            if !super::super::util::is_ident(raw) {
                bail!("Line {}: invalid env name '{}'", line_no, raw);
            }
            (raw, None)
        };
        service.env.push(crate::ir::EnvVar {
            name: name.to_string(),
            default,
        });
        return Ok(());
    }
    if content.starts_with("auth ") {
        service.auth = Some(parse_auth_decl(content, line_no)?);
        return Ok(());
    }
    if content.starts_with("middleware ") {
        let mw = parse_middleware_decl(content, line_no)?;
        service.middleware.push(mw);
        return Ok(());
    }
    if content.starts_with("socket ") {
        if let Some(route) = finalize_route(state.current_route.take(), line_no)? {
            service.routes.push(route);
        }
        let sb = parse_socket_decl(content, line_no)?;
        state.current_socket = Some(sb);
        return Ok(());
    }
    if let Some(route) = finalize_route(state.current_route.take(), line_no)? {
        service.routes.push(route);
    }
    service.block = ServiceBlock::None;
    let rb = parse_route_decl(content, line_no)?;
    state.current_route = Some(rb);
    Ok(())
}

pub(super) fn handle_indent8(
    state: &mut ProjectState,
    content: &str,
    line_no: usize,
) -> Result<()> {
    if let Some(rpc) = state.current_rpc.as_mut() {
        if rpc.block == super::super::rpc::RpcBlock::Input
            || rpc.block == super::super::rpc::RpcBlock::Headers
        {
            parse_rpc_input_field(rpc, content, line_no)?;
            return Ok(());
        }
    }
    if let Some(socket) = state.current_socket.as_mut() {
        if socket.block == super::super::socket::SocketBlock::Trigger {
            if let Some(trigger) = finalize_trigger(state.current_trigger.take(), line_no)? {
                socket.triggers.push(trigger);
            }
        }
        if let Some(trigger) = state.current_trigger.take() {
            if socket.block != super::super::socket::SocketBlock::Trigger {
                if let Some(trigger) = finalize_trigger(Some(trigger), line_no)? {
                    socket.triggers.push(trigger);
                }
            } else {
                state.current_trigger = Some(trigger);
            }
        }
        if content.starts_with("use ") {
            let module = parse_use_decl(content, line_no)?;
            socket.modules.push(module);
            return Ok(());
        }
        if content.starts_with("auth ") {
            if socket.auth.is_some() {
                bail!("Line {}: duplicate auth directive", line_no);
            }
            socket.auth = Some(parse_auth_decl(content, line_no)?);
            return Ok(());
        }
        if content.starts_with("middleware ") {
            let mw = parse_middleware_decl(content, line_no)?;
            socket.middleware.push(mw);
            return Ok(());
        }
        if content.starts_with("rate_limit ") {
            if socket.rate_limit.is_some() {
                bail!("Line {}: duplicate rate_limit", line_no);
            }
            let raw = content.strip_prefix("rate_limit ").unwrap().trim();
            socket.rate_limit = Some(super::super::route::parse_rate_limit(raw, line_no)?);
            return Ok(());
        }
        if content.starts_with("trigger ") {
            if let Some(trigger) = finalize_trigger(state.current_trigger.take(), line_no)? {
                socket.triggers.push(trigger);
            }
            parse_socket_body_line(socket, content, line_no)?;
            state.current_trigger = Some(parse_trigger_decl(content, line_no)?);
            return Ok(());
        }
        if content == "inbound:"
            || content == "outbound:"
            || content == "headers:"
            || content.starts_with("room ")
            || content.starts_with("topic ")
        {
            parse_socket_body_line(socket, content, line_no)?;
            return Ok(());
        }
    }
    if state.current_type.is_some() {
        bail!(
            "Line {}: type fields must be indented with 4 spaces",
            line_no
        );
    }
    if state.current_enum.is_some() {
        bail!(
            "Line {}: enum variants must be indented with 4 spaces",
            line_no
        );
    }
    if let Some(service) = state.current_service.as_mut() {
        match service.block {
            ServiceBlock::Config => {
                parse_service_config_line(service, content, line_no)?;
                return Ok(());
            }
            ServiceBlock::GrpcConfig => {
                parse_grpc_config_line(service, content, line_no)?;
                return Ok(());
            }
            ServiceBlock::EventsConfig => {
                parse_events_config_line(service, content, line_no)?;
                return Ok(());
            }
            ServiceBlock::Headers => {
                parse_header_field_line(&mut service.headers, content, line_no)?;
                return Ok(());
            }
            ServiceBlock::None => {}
        }
    }
    let rb = state
        .current_route
        .as_mut()
        .ok_or_else(|| anyhow!("Line {}: route block without a route header", line_no))?;
    parse_route_body_line(rb, content, line_no)
}

pub(super) fn handle_indent12(
    state: &mut ProjectState,
    content: &str,
    line_no: usize,
) -> Result<()> {
    if let Some(socket) = state.current_socket.as_mut() {
        if socket.block == super::super::socket::SocketBlock::Inbound
            || socket.block == super::super::socket::SocketBlock::Outbound
            || socket.block == super::super::socket::SocketBlock::Headers
        {
            parse_socket_message_line(socket, content, line_no)?;
            return Ok(());
        }
        if socket.block == super::super::socket::SocketBlock::Trigger {
            if let Some(trigger) = state.current_trigger.as_mut() {
                parse_trigger_body_line(trigger, socket, content, line_no)?;
                return Ok(());
            } else {
                bail!("Line {}: trigger block without trigger header", line_no);
            }
        }
    }
    if state.current_type.is_some() {
        bail!(
            "Line {}: type fields must be indented with 4 spaces",
            line_no
        );
    }
    if state.current_enum.is_some() {
        bail!(
            "Line {}: enum variants must be indented with 4 spaces",
            line_no
        );
    }
    let rb = state
        .current_route
        .as_mut()
        .ok_or_else(|| anyhow!("Line {}: input block without a route header", line_no))?;
    if rb.input_mode == InputMode::InHeaders {
        parse_header_field_line(&mut rb.headers, content, line_no)?;
    } else if rb.response_mode {
        let spec = parse_response_line(content, line_no)?;
        rb.responses.push(spec);
    } else {
        parse_input_section_line(rb, content, line_no)?;
    }
    Ok(())
}

pub(super) fn handle_indent16(
    state: &mut ProjectState,
    content: &str,
    line_no: usize,
) -> Result<()> {
    if state.current_type.is_some() {
        bail!(
            "Line {}: type fields must be indented with 4 spaces",
            line_no
        );
    }
    if state.current_enum.is_some() {
        bail!(
            "Line {}: enum variants must be indented with 4 spaces",
            line_no
        );
    }
    let rb = state
        .current_route
        .as_mut()
        .ok_or_else(|| anyhow!("Line {}: input field without a route header", line_no))?;
    parse_input_field_line(rb, content, line_no)
}

pub(super) fn finalize_eof(state: &mut ProjectState, line_no: usize) -> Result<()> {
    if let Some(td) = finalize_type(state.current_type.take(), line_no)? {
        state.global_types.push(td);
    }
    if let Some(ed) = finalize_enum(state.current_enum.take(), line_no)? {
        state.global_enums.push(ed);
    }
    if let Some(ev) = finalize_event(state.current_event.take(), line_no)? {
        state.global_events.push(ev);
    }
    if let Some(rpc) = finalize_rpc(state.current_rpc.take(), line_no)? {
        state.global_rpcs.push(rpc);
    }
    finalize_current_service(state, line_no)?;
    Ok(())
}

fn finalize_global_decls(state: &mut ProjectState, line_no: usize) -> Result<()> {
    if let Some(td) = finalize_type(state.current_type.take(), line_no)? {
        state.global_types.push(td);
    }
    if let Some(ed) = finalize_enum(state.current_enum.take(), line_no)? {
        state.global_enums.push(ed);
    }
    if let Some(ev) = finalize_event(state.current_event.take(), line_no)? {
        state.global_events.push(ev);
    }
    if let Some(rpc) = finalize_rpc(state.current_rpc.take(), line_no)? {
        state.global_rpcs.push(rpc);
    }
    Ok(())
}

fn finalize_current_service(state: &mut ProjectState, line_no: usize) -> Result<()> {
    if let Some(svc) = finalize_service(
        state.current_service.take(),
        state.current_route.take(),
        state.current_type.take(),
        state.current_enum.take(),
        state.current_event.take(),
        state.current_rpc.take(),
        state.current_socket.take(),
        state.current_trigger.take(),
        line_no,
        &state.global_types,
        &state.global_enums,
        &state.global_events,
        &state.global_rpcs,
    )? {
        state.services.push(svc);
    }
    Ok(())
}
