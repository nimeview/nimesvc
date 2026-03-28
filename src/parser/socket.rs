use anyhow::{Result, anyhow, bail};

use crate::ir::{CallSpec, SocketDef, SocketMessage, SocketTrigger};

use super::model::{SocketBuilder, SocketTriggerBuilder};
use super::types::parse_name_with_version;
use super::types::parse_type_with_validation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SocketBlock {
    None,
    Inbound,
    Outbound,
    Headers,
    Trigger,
}

pub(super) fn parse_socket_decl(content: &str, line_no: usize) -> Result<SocketBuilder> {
    let content = content.trim();
    if !content.starts_with("socket ") || !content.ends_with(':') {
        bail!("Line {}: expected `socket <Name> \"<path>\":`", line_no);
    }
    let raw = content
        .strip_prefix("socket ")
        .and_then(|s| s.strip_suffix(':'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid socket declaration", line_no))?;

    let (name_raw, path_raw) = raw
        .split_once(' ')
        .ok_or_else(|| anyhow!("Line {}: socket requires name and path", line_no))?;
    let name = parse_name_with_version(name_raw.trim(), line_no)?;
    let path = super::util::parse_quoted_value(path_raw.trim(), line_no)?;
    Ok(SocketBuilder {
        name,
        path,
        rooms: Vec::new(),
        triggers: Vec::new(),
        inbound: Vec::new(),
        outbound: Vec::new(),
        modules: Vec::new(),
        auth: None,
        middleware: Vec::new(),
        rate_limit: None,
        headers: Vec::new(),
        block: SocketBlock::None,
    })
}

pub(super) fn parse_socket_body_line(
    socket: &mut SocketBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let content = content.trim();
    if content.starts_with("room ") || content.starts_with("topic ") {
        parse_socket_room_line(socket, content, line_no)?;
        return Ok(());
    }
    if content == "inbound:" {
        socket.block = SocketBlock::Inbound;
        return Ok(());
    }
    if content == "outbound:" {
        socket.block = SocketBlock::Outbound;
        return Ok(());
    }
    if content == "headers:" {
        socket.block = SocketBlock::Headers;
        return Ok(());
    }
    if content.starts_with("trigger ") {
        socket.block = SocketBlock::Trigger;
        return Ok(());
    }
    bail!(
        "Line {}: expected `inbound:`, `outbound:`, `headers:`, `trigger <Name>:`, or `room \"...\"`",
        line_no
    );
}

pub(super) fn parse_socket_message_line(
    socket: &mut SocketBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    if socket.block != SocketBlock::Inbound
        && socket.block != SocketBlock::Outbound
        && socket.block != SocketBlock::Headers
        && socket.block != SocketBlock::Trigger
    {
        bail!("Line {}: socket message outside inbound/outbound", line_no);
    }
    if socket.block == SocketBlock::Headers {
        super::service::parse_header_field_line(&mut socket.headers, content, line_no)?;
        return Ok(());
    }
    if socket.block == SocketBlock::Trigger {
        bail!("Line {}: trigger requires its own block", line_no);
    }
    let content = content.trim();
    let (name_raw, handler_raw) = content
        .split_once("->")
        .ok_or_else(|| anyhow!("Line {}: socket message requires `-> handler`", line_no))?;
    let name = parse_name_with_version(name_raw.trim(), line_no)?;
    let handler = parse_socket_handler(handler_raw.trim(), line_no)?;
    let msg = SocketMessage { name, handler };
    if socket.block == SocketBlock::Inbound {
        socket.inbound.push(msg);
    } else {
        socket.outbound.push(msg);
    }
    Ok(())
}

fn is_reserved_socket_event(name: &str) -> bool {
    matches!(
        name,
        "Join"
            | "Exit"
            | "MessageIn"
            | "MessageOut"
            | "Typing"
            | "Ping"
            | "Pong"
            | "Auth"
            | "Subscribe"
            | "Unsubscribe"
            | "RoomJoin"
            | "RoomLeave"
            | "Ack"
            | "Receipt"
            | "UserJoined"
            | "UserLeft"
            | "Error"
            | "ServerNotice"
    )
}

pub(super) fn finalize_socket(
    socket: Option<SocketBuilder>,
    line_no: usize,
) -> Result<Option<SocketDef>> {
    let Some(socket) = socket else {
        return Ok(None);
    };
    if socket.inbound.is_empty() && socket.outbound.is_empty() {
        bail!(
            "Line {}: socket '{}' has no inbound or outbound messages",
            line_no,
            socket.name.display()
        );
    }
    let trigger_names: std::collections::HashSet<_> =
        socket.triggers.iter().map(|t| t.name.clone()).collect();
    for msg in socket.inbound.iter().chain(socket.outbound.iter()) {
        if is_reserved_socket_event(&msg.name.name) {
            continue;
        }
        if !trigger_names.contains(&msg.name) {
            bail!(
                "Line {}: socket message '{}' is not a reserved event or trigger",
                line_no,
                msg.name.display()
            );
        }
    }
    Ok(Some(SocketDef {
        name: socket.name,
        path: socket.path,
        rooms: socket.rooms,
        triggers: socket.triggers,
        inbound: socket.inbound,
        outbound: socket.outbound,
        modules: socket.modules,
        auth: socket.auth,
        middleware: socket.middleware,
        rate_limit: socket.rate_limit,
        headers: socket.headers,
    }))
}

pub(super) fn parse_socket_room_line(
    socket: &mut SocketBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let raw = if let Some(rest) = content.strip_prefix("room ") {
        rest.trim()
    } else if let Some(rest) = content.strip_prefix("topic ") {
        rest.trim()
    } else {
        bail!("Line {}: invalid room/topic", line_no);
    };
    let name = super::util::parse_quoted_value(raw, line_no)?;
    if !socket.rooms.contains(&name) {
        socket.rooms.push(name);
    }
    Ok(())
}

pub(super) fn parse_trigger_decl(content: &str, line_no: usize) -> Result<SocketTriggerBuilder> {
    let content = content.trim();
    if !content.starts_with("trigger ") || !content.ends_with(':') {
        bail!("Line {}: expected `trigger <Name>:`", line_no);
    }
    let name = content
        .strip_prefix("trigger ")
        .and_then(|s| s.strip_suffix(':'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid trigger name", line_no))?;
    let name = parse_name_with_version(name, line_no)?;
    if is_reserved_socket_event(&name.name) {
        bail!("Line {}: trigger '{}' is reserved", line_no, name.display());
    }
    Ok(SocketTriggerBuilder {
        name,
        room: None,
        payload: None,
    })
}

pub(super) fn parse_trigger_body_line(
    trigger: &mut SocketTriggerBuilder,
    socket: &SocketBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let content = content.trim();
    if content.starts_with("room ") || content.starts_with("topic ") {
        let raw = if let Some(rest) = content.strip_prefix("room ") {
            rest.trim()
        } else {
            content.strip_prefix("topic ").unwrap().trim()
        };
        let room = super::util::parse_quoted_value(raw, line_no)?;
        if !socket.rooms.is_empty() && !socket.rooms.contains(&room) {
            bail!(
                "Line {}: trigger room '{}' is not declared in socket",
                line_no,
                room
            );
        }
        trigger.room = Some(room);
        return Ok(());
    }
    if content.starts_with("payload") {
        let raw = content
            .trim_start_matches("payload")
            .trim_start_matches(':')
            .trim();
        if raw.is_empty() {
            bail!("Line {}: trigger payload missing type", line_no);
        }
        let (ty, _) = parse_type_with_validation(raw, line_no)?;
        trigger.payload = Some(ty);
        return Ok(());
    }
    bail!(
        "Line {}: expected `room \"...\"` or `payload <Type>`",
        line_no
    );
}

pub(super) fn finalize_trigger(
    trigger: Option<SocketTriggerBuilder>,
    line_no: usize,
) -> Result<Option<SocketTrigger>> {
    let Some(trigger) = trigger else {
        return Ok(None);
    };
    let payload = trigger.payload.ok_or_else(|| {
        anyhow!(
            "Line {}: trigger '{}' missing payload",
            line_no,
            trigger.name.display()
        )
    })?;
    Ok(Some(SocketTrigger {
        name: trigger.name,
        room: trigger.room,
        payload,
    }))
}

fn parse_socket_handler(raw: &str, line_no: usize) -> Result<CallSpec> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("Line {}: empty socket handler", line_no);
    }
    let (is_async, raw) = if raw.starts_with("async ") {
        (true, raw.trim_start_matches("async ").trim())
    } else if raw.starts_with("sync ") {
        (false, raw.trim_start_matches("sync ").trim())
    } else {
        (false, raw)
    };
    let parts: Vec<&str> = raw.split('.').collect();
    if parts.len() != 2 {
        bail!("Line {}: handler must be module.func", line_no);
    }
    let module = parts[0].trim();
    let function = parts[1].trim();
    if !super::util::is_ident(module) || !super::util::is_ident(function) {
        bail!("Line {}: invalid handler '{}'", line_no, raw);
    }
    Ok(CallSpec {
        service: None,
        service_base: None,
        module: module.to_string(),
        function: function.to_string(),
        args: Vec::new(),
        is_async,
    })
}
