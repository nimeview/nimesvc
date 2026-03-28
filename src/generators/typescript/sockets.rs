use crate::generators::common::headers::header_runtime_key;
use crate::ir::{AuthSpec, CallSpec, Service, SocketDef};

pub(super) fn render_socket_helpers(service: &Service) -> String {
    if service.sockets.sockets.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(
        r#"
type SocketFrame = { type: string; data: any; room?: string };

class SocketRateLimit {
  private count = 0;
  private reset = Date.now();
  constructor(private max: number, private windowMs: number) {}
  allow(): boolean {
    const now = Date.now();
    if (now - this.reset >= this.windowMs) {
      this.reset = now;
      this.count = 0;
    }
    if (this.count >= this.max) return false;
    this.count += 1;
    return true;
  }
}

"#,
    );
    for socket in &service.sockets.sockets {
        out.push_str(&render_socket_helper(service, socket));
        out.push('\n');
    }
    out
}

fn render_socket_helper(service: &Service, socket: &SocketDef) -> String {
    let ctx_name = format!("{}SocketContext", socket.name.code_name());
    let room_map = format!("wsRooms{}", socket.name.code_name());
    let room_allowed = format!("wsRoomAllowed{}", socket.name.code_name());
    let room_helpers = if socket.rooms.is_empty() {
        format!(
            "const {room_map} = new Map<string, Set<WebSocket>>();\nfunction {room_allowed}(_room: string) {{ return true; }}\n",
            room_map = room_map,
            room_allowed = room_allowed
        )
    } else {
        let rooms = socket
            .rooms
            .iter()
            .map(|r| format!("\"{}\"", r))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "const {room_map} = new Map<string, Set<WebSocket>>();\nconst {room_allowed}Set = new Set([{rooms}]);\nfunction {room_allowed}(room: string) {{ return {room_allowed}Set.has(room); }}\n",
            room_map = room_map,
            room_allowed = room_allowed,
            rooms = rooms
        )
    };
    let mut outbound_type_fields = String::new();
    let mut outbound_methods = String::new();
    for msg in &socket.outbound {
        outbound_type_fields.push_str(&format!(
            "  send{msg}: (data: any) => void;\n",
            msg = msg.name.code_name()
        ));
        outbound_methods.push_str(&format!(
            "    send{msg}: (data) => {{ ctx.sendRaw('{msg}', data); }},\n",
            msg = msg.name.code_name()
        ));
    }
    outbound_type_fields.push_str("  joinRoom: (room: string) => void;\n  leaveRoom: (room: string) => void;\n  sendRoom: (room: string, kind: string, data: any) => void;\n  getRoom: () => string | undefined;\n");
    outbound_methods.push_str(&format!(
        "    joinRoom: (room) => {{\n      if (!{room_allowed}(room)) {{ ctx.sendError('invalid room'); return; }}\n      let set = {room_map}.get(room);\n      if (!set) {{ set = new Set(); {room_map}.set(room, set); }}\n      set.add(socket);\n    }},\n    leaveRoom: (room) => {{\n      const set = {room_map}.get(room);\n      if (set) {{ set.delete(socket); if (set.size === 0) {room_map}.delete(room); }}\n    }},\n    sendRoom: (room, kind, data) => {{\n      if (!{room_allowed}(room)) {{ ctx.sendError('invalid room'); return; }}\n      const set = {room_map}.get(room);\n      if (!set) return;\n      const frame: SocketFrame = {{ type: kind, data, room }};\n      const text = JSON.stringify(frame);\n      for (const client of set) client.send(text);\n    }},\n    getRoom: () => currentRoom,\n",
        room_allowed = room_allowed,
        room_map = room_map
    ));
    let mut dispatch = String::new();
    let mut has_join = false;
    let mut has_exit = false;
    let mut has_ping = false;
    for msg in &socket.inbound {
        let call = render_socket_call(&msg.handler);
        let required_room = socket
            .triggers
            .iter()
            .find(|t| t.name == msg.name)
            .and_then(|t| t.room.as_deref());
        let room_guard = if let Some(room) = required_room {
            format!(
                "        if (frame.room !== '{room}') {{ ctx.sendError('invalid room'); return; }}\n",
                room = room
            )
        } else {
            String::new()
        };
        if msg.name.name == "Join" {
            has_join = true;
        }
        if msg.name.name == "Exit" {
            has_exit = true;
        }
        if msg.name.name == "Ping" {
            has_ping = true;
        }
        if msg.name.name == "Ping" {
            dispatch.push_str(&format!(
                "      case 'Ping':\n        ctx.sendRaw('Pong', {{}});\n{room_guard}        {call};\n        break;\n",
                call = call
            ));
        } else {
            dispatch.push_str(&format!(
                "      case '{name}':\n{room_guard}        {call};\n        break;\n",
                name = msg.name.code_name(),
                call = call,
                room_guard = room_guard
            ));
        }
    }
    if dispatch.is_empty() {
        dispatch.push_str("      default:\n        ctx.sendError('unknown message');\n");
    } else {
        dispatch.push_str("      default:\n        ctx.sendError('unknown message');\n");
    }
    let auth_check = render_socket_auth_check(socket);
    let header_check = render_socket_header_check(socket);
    let middleware_calls = render_socket_middleware_calls(socket);
    let rate_limit = render_socket_rate_limit(socket);
    let middleware_defs = render_socket_middleware_defs(socket);
    let join_call = if has_join {
        format!(
            "  const joinFrame: SocketFrame = {{ type: 'Join', data: {{}} }};\n  const frame = joinFrame;\n  {call};\n",
            call = render_socket_call(
                &socket
                    .inbound
                    .iter()
                    .find(|m| m.name.name == "Join")
                    .unwrap()
                    .handler
            )
        )
    } else {
        String::new()
    };
    let exit_call = if has_exit {
        format!(
            "  const exitFrame: SocketFrame = {{ type: 'Exit', data: {{}} }};\n  const frame = exitFrame;\n  {call};\n",
            call = render_socket_call(
                &socket
                    .inbound
                    .iter()
                    .find(|m| m.name.name == "Exit")
                    .unwrap()
                    .handler
            )
        )
    } else {
        String::new()
    };
    let ping_builtin = if has_ping {
        String::new()
    } else {
        "      case 'Ping':\n        ctx.sendRaw('Pong', {});\n        break;\n".to_string()
    };

    format!(
        r#"{room_helpers}
type {ctx_name} = {{
  headers: Record<string, string | undefined>;
  sendRaw: (kind: string, data: any) => void;
  sendError: (message: string) => void;
{outbound_type_fields}}};

const ws{sock} = new WebSocketServer({{ server, path: "{path}" }});
ws{sock}.on('connection', (socket, req) => {{
  let currentRoom: string | undefined = undefined;
  const headers: Record<string, string | undefined> = {{}};
  for (const [key, value] of Object.entries(req.headers)) {{
    headers[key.toLowerCase()] = Array.isArray(value) ? value[0] : value;
  }}
  const ctx: {ctx_name} = {{
    headers,
    sendRaw: (kind, data) => {{
      const frame: SocketFrame = {{ type: kind, data }};
      socket.send(JSON.stringify(frame));
    }},
    sendError: (message) => {{
      const frame: SocketFrame = {{ type: 'Error', data: {{ message }} }};
      socket.send(JSON.stringify(frame));
    }},
{outbound_methods}  }};
  console.error('[{service}] WS connect {path} -> OPEN');

{auth_check}{header_check}{rate_limit}{join_call}
  socket.on('message', async (raw) => {{
    let frame: SocketFrame;
    try {{
      frame = JSON.parse(raw.toString());
    }} catch {{
      console.error('[{service}] WS message {path} -> INVALID_MESSAGE');
      ctx.sendError('invalid message');
      return;
    }}
    if (frame.room && !{room_allowed}(frame.room)) {{
      console.error('[{service}] WS message {path} -> INVALID_ROOM');
      ctx.sendError('invalid room');
      return;
    }}
    console.error('[{service}] WS message {path} -> type=' + frame.type + ' room=' + (frame.room ?? '-'));
    currentRoom = frame.room;
{middleware_calls}    switch (frame.type) {{
{ping_builtin}{dispatch}    }}
  }});
  socket.on('close', async () => {{
{exit_call}    console.error('[{service}] WS disconnect {path} -> CLOSED');
    for (const [room, set] of {room_map}.entries()) {{
      set.delete(socket);
      if (set.size === 0) {room_map}.delete(room);
    }}
  }});
}});

{middleware_defs}
"#,
        ctx_name = ctx_name,
        sock = socket.name.code_name(),
        path = socket.path,
        outbound_methods = outbound_methods,
        outbound_type_fields = outbound_type_fields,
        room_helpers = room_helpers,
        room_allowed = room_allowed,
        room_map = room_map,
        auth_check = auth_check,
        header_check = header_check,
        rate_limit = rate_limit,
        middleware_calls = middleware_calls,
        dispatch = dispatch,
        join_call = join_call,
        exit_call = exit_call,
        ping_builtin = ping_builtin,
        middleware_defs = middleware_defs,
        service = service.name
    )
}

fn render_socket_call(call: &CallSpec) -> String {
    let module = &call.module;
    let func = &call.function;
    if call.is_async {
        format!("await {}.{}(ctx, frame.data)", module, func)
    } else {
        format!("{}.{}(ctx, frame.data)", module, func)
    }
}

fn render_socket_auth_check(socket: &SocketDef) -> String {
    let Some(auth) = socket.auth.as_ref() else {
        return String::new();
    };
    match auth {
        AuthSpec::Bearer => {
            "  if (!headers['authorization']) { ctx.sendError('missing authorization'); socket.close(); return; }\n".to_string()
        }
        AuthSpec::ApiKey => {
            "  if (!headers['x-api-key']) { ctx.sendError('missing api key'); socket.close(); return; }\n".to_string()
        }
        AuthSpec::None => String::new(),
    }
}

fn render_socket_header_check(socket: &SocketDef) -> String {
    if socket.headers.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for field in &socket.headers {
        if field.optional {
            continue;
        }
        let key = header_runtime_key(&field.name);
        out.push_str(&format!(
            "  if (!headers['{name}']) {{ ctx.sendError('missing header {name}'); socket.close(); return; }}\n",
            name = key
        ));
    }
    out
}

fn render_socket_rate_limit(socket: &SocketDef) -> String {
    let Some(limit) = socket.rate_limit.as_ref() else {
        return String::new();
    };
    format!(
        "  const rateLimit{code} = new SocketRateLimit({max}, {window} * 1000);\n",
        code = socket.name.code_name(),
        max = limit.max,
        window = limit.per_seconds
    )
}

fn render_socket_middleware_calls(socket: &SocketDef) -> String {
    let mut out = String::new();
    for name in &socket.middleware {
        out.push_str(&format!(
            "    try {{ await ws_{name}(ctx, frame.type); }} catch (err: any) {{ ctx.sendError(String(err?.message || err)); return; }}\n",
            name = name
        ));
    }
    if socket.rate_limit.is_some() {
        out.push_str(&format!(
            "    if (!rateLimit{code}.allow()) {{ ctx.sendError('rate limited'); return; }}\n",
            code = socket.name.code_name()
        ));
    }
    out
}

fn render_socket_middleware_defs(socket: &SocketDef) -> String {
    if socket.middleware.is_empty() {
        return String::new();
    }
    let ctx_name = format!("{}SocketContext", socket.name.code_name());
    let mut out = String::new();
    for name in &socket.middleware {
        out.push_str(&format!(
            "async function ws_{name}(_ctx: {ctx}, message: string): Promise<void> {{\n  const kind = message.trim();\n  if (!kind) {{\n    throw new Error('empty socket message kind');\n  }}\n  if (kind.length > 128) {{\n    throw new Error('socket message kind is too long');\n  }}\n}}\n",
            name = name,
            ctx = ctx_name
        ));
    }
    out
}
