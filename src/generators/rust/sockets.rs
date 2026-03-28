use crate::generators::common::headers::header_runtime_key;
use crate::ir::{Service, SocketDef};

use super::routes::render_socket_call;
use super::util::to_snake_case;

pub(super) fn socket_handler_name(name: &str) -> String {
    format!("ws_{}", to_snake_case(name))
}

pub(super) fn render_socket_helpers(service: &Service) -> String {
    if service.sockets.sockets.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(
        r#"
#[derive(Debug, Serialize, Deserialize)]
struct SocketFrame {
    #[serde(rename = "type")]
    kind: String,
    data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    room: Option<String>,
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
    let handler_name = socket_handler_name(&socket.name.code_name());
    let handle_fn = format!("handle_{}", handler_name);
    let room_static = format!("SOCKET_ROOMS_{}", socket.name.code_name().to_uppercase());
    let id_static = format!("SOCKET_CONN_ID_{}", socket.name.code_name().to_uppercase());
    let room_fn = format!("socket_rooms_{}", to_snake_case(&socket.name.code_name()));
    let allowed_fn = format!(
        "socket_room_allowed_{}",
        to_snake_case(&socket.name.code_name())
    );
    let (allowed_match, room_arg) = if socket.rooms.is_empty() {
        ("true".to_string(), "_room")
    } else {
        let mut parts = String::new();
        for (idx, room) in socket.rooms.iter().enumerate() {
            if idx > 0 {
                parts.push_str(" | ");
            }
            parts.push_str(&format!("\"{}\"", room));
        }
        (format!("matches!(room, {})", parts), "room")
    };
    let mut outbound_methods = String::new();
    for msg in &socket.outbound {
        outbound_methods.push_str(&format!(
            "    pub fn send_{name}(&self, data: Value) {{\n        self.send_raw(\"{name}\", data);\n    }}\n",
            name = msg.name.code_name()
        ));
    }
    outbound_methods.push_str(&format!(
        "    pub fn send_room(&self, room: &str, kind: &str, data: Value) {{\n        if !{allowed_fn}(room) {{\n            self.send_error(\"invalid room\");\n            return;\n        }}\n        let rooms = {room_fn}();\n        let map = rooms.lock().unwrap();\n        if let Some(members) = map.get(room) {{\n            let frame = SocketFrame {{ kind: kind.to_string(), data, room: Some(room.to_string()) }};\n            if let Ok(text) = serde_json::to_string(&frame) {{\n                for sender in members.values() {{\n                    let _ = sender.send(Message::Text(text.clone()));\n                }}\n            }}\n        }}\n    }}\n    pub fn join_room(&self, room: &str) {{\n        if !{allowed_fn}(room) {{\n            self.send_error(\"invalid room\");\n            return;\n        }}\n        let rooms = {room_fn}();\n        let mut map = rooms.lock().unwrap();\n        let entry = map.entry(room.to_string()).or_insert_with(HashMap::new);\n        entry.insert(self.id, self.sender.clone());\n    }}\n    pub fn leave_room(&self, room: &str) {{\n        let rooms = {room_fn}();\n        let mut map = rooms.lock().unwrap();\n        if let Some(entry) = map.get_mut(room) {{\n            entry.remove(&self.id);\n            if entry.is_empty() {{\n                map.remove(room);\n            }}\n        }}\n    }}\n    pub fn leave_all_rooms(&self) {{\n        let rooms = {room_fn}();\n        let mut map = rooms.lock().unwrap();\n        let keys: Vec<String> = map.keys().cloned().collect();\n        for room in keys {{\n            if let Some(entry) = map.get_mut(&room) {{\n                entry.remove(&self.id);\n                if entry.is_empty() {{\n                    map.remove(&room);\n                }}\n            }}\n        }}\n    }}\n",
        allowed_fn = allowed_fn,
        room_fn = room_fn
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
                "                if frame.room.as_deref() != Some(\"{room}\") {{\n                    ctx.send_error(\"invalid room\");\n                    continue;\n                }}\n",
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
                "            \"Ping\" => {{\n                ctx.send_raw(\"Pong\", json!({{}}));\n{room_guard}                {call};\n            }}\n",
                call = call
            ));
        } else {
            dispatch.push_str(&format!(
                "            \"{name}\" => {{\n{room_guard}                {call};\n            }}\n",
                name = msg.name.code_name(),
                call = call,
                room_guard = room_guard
            ));
        }
    }
    if dispatch.is_empty() {
        dispatch.push_str("            _ => {\n                ctx.send_error(\"unknown message\");\n            }\n");
    } else {
        dispatch.push_str("            _ => {\n                ctx.send_error(\"unknown message\");\n            }\n");
    }

    let auth_check = render_socket_auth_check(socket);
    let header_check = render_socket_header_check(socket);
    let middleware_calls = render_socket_middleware_calls(socket);
    let rate_limit_check = render_socket_rate_limit_check(socket);
    let middleware_defs = render_socket_middleware_defs(socket);
    let rate_limit_defs = render_socket_rate_limit_defs(socket);
    let join_call = if has_join {
        format!(
            "    let frame = SocketFrame {{ kind: \"Join\".to_string(), data: json!({{}}), room: None }};\n    {call};\n",
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
            "    let frame = SocketFrame {{ kind: \"Exit\".to_string(), data: json!({{}}), room: None }};\n    {call};\n",
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
        "            \"Ping\" => { ctx.send_raw(\"Pong\", json!({})); }\n".to_string()
    };

    let room_helpers = format!(
        r#"
static {room_static}: OnceLock<Mutex<HashMap<String, HashMap<u64, mpsc::UnboundedSender<Message>>>>> = OnceLock::new();
static {id_static}: AtomicU64 = AtomicU64::new(1);

fn {room_fn}() -> &'static Mutex<HashMap<String, HashMap<u64, mpsc::UnboundedSender<Message>>>> {{
    {room_static}.get_or_init(|| Mutex::new(HashMap::new()))
}}

fn {allowed_fn}({room_arg}: &str) -> bool {{
    {allowed_match}
}}
"#,
        room_static = room_static,
        id_static = id_static,
        room_fn = room_fn,
        allowed_fn = allowed_fn,
        allowed_match = allowed_match,
        room_arg = room_arg
    );

    format!(
        r#"
{room_helpers}
#[derive(Clone)]
struct {ctx_name} {{
    headers: HeaderMap,
    sender: mpsc::UnboundedSender<Message>,
    id: u64,
    room: Arc<Mutex<Option<String>>>,
}}

#[allow(dead_code, non_snake_case)]
impl {ctx_name} {{
    fn set_room(&self, room: Option<String>) {{
        *self.room.lock().unwrap() = room;
    }}

    fn room(&self) -> Option<String> {{
        self.room.lock().unwrap().clone()
    }}

    fn send_raw(&self, kind: &str, data: Value) {{
        let frame = SocketFrame {{ kind: kind.to_string(), data, room: None }};
        if let Ok(text) = serde_json::to_string(&frame) {{
            let _ = self.sender.send(Message::Text(text));
        }}
    }}

    fn send_error(&self, message: &str) {{
        self.send_raw("Error", json!({{ "message": message }}));
    }}

{outbound_methods}}}

async fn {handler_name}(ws: WebSocketUpgrade, headers: HeaderMap) -> impl IntoResponse {{
    ws.on_upgrade(move |socket| {handle_fn}(socket, headers))
}}

async fn {handle_fn}(socket: WebSocket, headers: HeaderMap) {{
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let id = {id_static}.fetch_add(1, Ordering::Relaxed);
    let ctx = {ctx_name} {{
        headers: headers.clone(),
        sender: tx,
        id,
        room: Arc::new(Mutex::new(None)),
    }};
    eprintln!("[{service}] WS connect {path} -> OPEN");

    let send_task = tokio::spawn(async move {{
        while let Some(msg) = rx.recv().await {{
            let _ = ws_sender.send(msg).await;
        }}
    }});

{auth_check}{header_check}{join_call}
    while let Some(Ok(msg)) = ws_receiver.next().await {{
        if let Message::Text(text) = msg {{
            let parsed: Result<SocketFrame, _> = serde_json::from_str(&text);
            let frame = match parsed {{
                Ok(frame) => frame,
                Err(_) => {{
                    eprintln!("[{service}] WS message {path} -> INVALID_MESSAGE");
                    ctx.send_error("invalid message");
                    continue;
                }}
            }};
            if let Some(room) = frame.room.as_deref() {{
                if !{allowed_fn}(room) {{
                    eprintln!("[{service}] WS message {path} -> INVALID_ROOM");
                    ctx.send_error("invalid room");
                    continue;
                }}
            }}
            eprintln!(
                "[{service}] WS message {path} -> type={{}} room={{}}",
                frame.kind,
                frame.room.as_deref().unwrap_or("-")
            );
            ctx.set_room(frame.room.clone());
{middleware_calls}{rate_limit_check}            match frame.kind.as_str() {{
{ping_builtin}{dispatch}            }}
        }}
    }}
{exit_call}
    eprintln!("[{service}] WS disconnect {path} -> CLOSED");
    ctx.leave_all_rooms();
    drop(ctx);
    let _ = send_task.abort();
}}

{middleware_defs}
{rate_limit_defs}
"#,
        ctx_name = ctx_name,
        handler_name = handler_name,
        handle_fn = handle_fn,
        outbound_methods = outbound_methods,
        dispatch = dispatch,
        auth_check = auth_check,
        header_check = header_check,
        middleware_calls = middleware_calls,
        rate_limit_check = rate_limit_check,
        join_call = join_call,
        exit_call = exit_call,
        ping_builtin = ping_builtin,
        middleware_defs = middleware_defs,
        rate_limit_defs = rate_limit_defs,
        room_helpers = room_helpers,
        allowed_fn = allowed_fn,
        id_static = id_static,
        service = service.name,
        path = socket.path
    )
}

fn render_socket_auth_check(socket: &SocketDef) -> String {
    let Some(auth) = socket.auth.as_ref() else {
        return String::new();
    };
    match auth {
        crate::ir::AuthSpec::Bearer => {
            "    if header_str(&headers, \"authorization\").is_none() {\n        let ctx = ctx.clone();\n        ctx.send_error(\"missing authorization\");\n        return;\n    }\n".to_string()
        }
        crate::ir::AuthSpec::ApiKey => {
            "    if header_str(&headers, \"x-api-key\").is_none() {\n        let ctx = ctx.clone();\n        ctx.send_error(\"missing api key\");\n        return;\n    }\n".to_string()
        }
        crate::ir::AuthSpec::None => String::new(),
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
            "    if header_str(&headers, \"{name}\").is_none() {{\n        let ctx = ctx.clone();\n        ctx.send_error(\"missing header {name}\");\n        return;\n    }}\n",
            name = key
        ));
    }
    out
}

fn render_socket_middleware_calls(socket: &SocketDef) -> String {
    if socket.middleware.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for name in &socket.middleware {
        out.push_str(&format!(
            "            if let Err(msg) = ws_{name}(&ctx, &frame.kind).await {{\n                ctx.send_error(&msg);\n                continue;\n            }}\n",
            name = name
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
            "async fn ws_{name}(_ctx: &{ctx}, message: &str) -> Result<(), String> {{\n    let message = message.trim();\n    if message.is_empty() {{\n        return Err(\"empty socket message kind\".to_string());\n    }}\n    if message.len() > 128 {{\n        return Err(\"socket message kind is too long\".to_string());\n    }}\n    Ok(())\n}}\n\n",
            name = name,
            ctx = ctx_name
        ));
    }
    out
}

fn render_socket_rate_limit_defs(socket: &SocketDef) -> String {
    if socket.rate_limit.is_none() {
        return String::new();
    };
    let static_name = format!("SOCKET_RATE_{}", socket.name.code_name().to_uppercase());
    format!(
        "static {static_name}: OnceLock<Mutex<(u32, Instant)>> = OnceLock::new();\n\n",
        static_name = static_name
    )
}

fn render_socket_rate_limit_check(socket: &SocketDef) -> String {
    let Some(limit) = socket.rate_limit.as_ref() else {
        return String::new();
    };
    let static_name = format!("SOCKET_RATE_{}", socket.name.code_name().to_uppercase());
    format!(
        "            let state = {static_name}.get_or_init(|| Mutex::new((0, Instant::now())));\n            let mut guard = state.lock().unwrap();\n            let elapsed = guard.1.elapsed();\n            if elapsed >= Duration::from_secs({window}) {{\n                *guard = (0, Instant::now());\n            }}\n            if guard.0 >= {max} {{\n                ctx.send_error(\"rate limited\");\n                continue;\n            }}\n            guard.0 += 1;\n            drop(guard);\n",
        static_name = static_name,
        max = limit.max,
        window = limit.per_seconds
    )
}
