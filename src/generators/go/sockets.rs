use crate::generators::common::headers::header_runtime_key;
use crate::ir::{CallSpec, Service, SocketDef};

use super::util::to_go_func_name;

pub(super) fn socket_handler_name(raw: &str) -> String {
    format!("ws{}", to_go_func_name(raw))
}

pub(super) fn render_socket_helpers(service: &Service) -> String {
    if service.sockets.sockets.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(
        r#"type socketFrameIn struct {
    Type string `json:"type"`
    Data json.RawMessage `json:"data"`
    Room string `json:"room,omitempty"`
}

var wsUpgrader = websocket.Upgrader{
    CheckOrigin: func(r *http.Request) bool { return true },
}

"#,
    );
    if service
        .sockets
        .sockets
        .iter()
        .any(|s| s.rate_limit.is_some())
    {
        out.push_str(
            "type rateLimit struct { count int; reset time.Time; max int; window time.Duration }\nfunc (r *rateLimit) Allow() bool {\n    now := time.Now()\n    if r.reset.IsZero() || now.After(r.reset) {\n        r.reset = now.Add(r.window)\n        r.count = 0\n    }\n    if r.count >= r.max { return false }\n    r.count += 1\n    return true\n}\n\n",
        );
    }
    for socket in &service.sockets.sockets {
        out.push_str(&render_socket_helper(service, socket));
        out.push('\n');
    }
    out
}

fn render_socket_helper(service: &Service, socket: &SocketDef) -> String {
    let ctx_name = format!("{}SocketContext", socket.name.code_name());
    let handler_name = socket_handler_name(&socket.name.code_name());
    let room_reg = format!("socketRooms{}", socket.name.code_name());
    let room_allowed = format!("socketRoomAllowed{}", socket.name.code_name());
    let allowed_switch = if socket.rooms.is_empty() {
        "    return true\n".to_string()
    } else {
        let mut cases = String::new();
        for room in &socket.rooms {
            cases.push_str(&format!(
                "    case \"{room}\":\n        return true\n",
                room = room
            ));
        }
        format!(
            "    switch room {{\n{cases}    default:\n        return false\n    }}\n",
            cases = cases
        )
    };
    let mut outbound_methods = String::new();
    for msg in &socket.outbound {
        outbound_methods.push_str(&format!(
            "func (ctx *{ctx}) Send{name}(data any) error {{\n    return ctx.SendRaw(\"{name}\", data)\n}}\n\n",
            ctx = ctx_name,
            name = msg.name.code_name()
        ));
    }
    outbound_methods.push_str(&format!(
        "func (ctx *{ctx}) SendRoom(room string, kind string, data any) error {{\n    if !{allowed}(room) {{\n        return ctx.SendError(\"invalid room\")\n    }}\n    frame := map[string]any{{\"type\": kind, \"data\": data, \"room\": room}}\n    rooms := &{room_reg}\n    rooms.Lock()\n    members := rooms.m[room]\n    rooms.Unlock()\n    for conn := range members {{\n        _ = conn.WriteJSON(frame)\n    }}\n    return nil\n}}\n\nfunc (ctx *{ctx}) JoinRoom(room string) {{\n    if !{allowed}(room) {{\n        _ = ctx.SendError(\"invalid room\")\n        return\n    }}\n    rooms := &{room_reg}\n    rooms.Lock()\n    members, ok := rooms.m[room]\n    if !ok {{\n        members = map[*websocket.Conn]bool{{}}\n        rooms.m[room] = members\n    }}\n    members[ctx.Conn] = true\n    rooms.Unlock()\n}}\n\nfunc (ctx *{ctx}) LeaveRoom(room string) {{\n    rooms := &{room_reg}\n    rooms.Lock()\n    if members, ok := rooms.m[room]; ok {{\n        delete(members, ctx.Conn)\n        if len(members) == 0 {{\n            delete(rooms.m, room)\n        }}\n    }}\n    rooms.Unlock()\n}}\n\nfunc (ctx *{ctx}) LeaveAllRooms() {{\n    rooms := &{room_reg}\n    rooms.Lock()\n    for room, members := range rooms.m {{\n        delete(members, ctx.Conn)\n        if len(members) == 0 {{\n            delete(rooms.m, room)\n        }}\n    }}\n    rooms.Unlock()\n}}\n\n",
        ctx = ctx_name,
        allowed = room_allowed,
        room_reg = room_reg
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
                "            if frame.Room != \"{room}\" {{\n                _ = ctx.SendError(\"invalid room\")\n                continue\n            }}\n",
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
                "        case \"Ping\":\n            _ = ctx.SendRaw(\"Pong\", map[string]any{{}})\n{room_guard}            {call}\n",
                call = call
            ));
        } else {
            dispatch.push_str(&format!(
                "        case \"{name}\":\n{room_guard}            {call}\n",
                name = msg.name.code_name(),
                call = call,
                room_guard = room_guard
            ));
        }
    }
    if dispatch.is_empty() {
        dispatch.push_str("        default:\n            _ = ctx.SendError(\"unknown message\")\n");
    } else {
        dispatch.push_str("        default:\n            _ = ctx.SendError(\"unknown message\")\n");
    }

    let auth_check = render_socket_auth_check(socket);
    let header_check = render_socket_header_check(socket);
    let middleware_calls = render_socket_middleware_calls(socket);
    let rate_limit_check = render_socket_rate_limit_check(socket);
    let middleware_defs = render_socket_middleware_defs(socket);
    let rate_limit_defs = render_socket_rate_limit_defs(socket);
    let join_call = if has_join {
        format!(
            "    {{\n        frame := socketFrameIn{{Type: \"Join\", Data: json.RawMessage(`{{}}`)}}\n        {call}\n    }}\n",
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
            "    {{\n        frame := socketFrameIn{{Type: \"Exit\", Data: json.RawMessage(`{{}}`)}}\n        {call}\n    }}\n",
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
        "        case \"Ping\":\n            _ = ctx.SendRaw(\"Pong\", map[string]any{})\n"
            .to_string()
    };

    let room_helpers = format!(
        "var {room_reg} = struct {{ sync.Mutex; m map[string]map[*websocket.Conn]bool }}{{m: map[string]map[*websocket.Conn]bool{{}}}}\n\nfunc {room_allowed}(room string) bool {{\n{allowed}\n}}\n\n",
        room_reg = room_reg,
        room_allowed = room_allowed,
        allowed = allowed_switch
    );

    format!(
        r#"type {ctx_name} struct {{
    Conn *websocket.Conn
    Headers http.Header
    Room string
}}

func (ctx *{ctx_name}) SendRaw(kind string, data any) error {{
    frame := map[string]any{{"type": kind, "data": data}}
    return ctx.Conn.WriteJSON(frame)
}}

func (ctx *{ctx_name}) SendError(message string) error {{
    return ctx.SendRaw("Error", map[string]any{{"message": message}})
}}

{room_helpers}{outbound_methods}
func {handler_name}(w http.ResponseWriter, r *http.Request) {{
    conn, err := wsUpgrader.Upgrade(w, r, nil)
    if err != nil {{
        return
    }}
    defer conn.Close()

    ctx := &{ctx_name}{{Conn: conn, Headers: r.Header}}
    fmt.Fprintf(os.Stderr, "[{service}] WS connect {path} -> OPEN\n")

{auth_check}{header_check}{join_call}
    for {{
        _, msg, err := conn.ReadMessage()
        if err != nil {{
            break
        }}
        var frame socketFrameIn
        if err := json.Unmarshal(msg, &frame); err != nil {{
            fmt.Fprintf(os.Stderr, "[{service}] WS message {path} -> INVALID_MESSAGE\n")
            _ = ctx.SendError("invalid message")
            continue
        }}
        if frame.Room != "" && !{room_allowed}(frame.Room) {{
            fmt.Fprintf(os.Stderr, "[{service}] WS message {path} -> INVALID_ROOM\n")
            _ = ctx.SendError("invalid room")
            continue
        }}
        fmt.Fprintf(os.Stderr, "[{service}] WS message {path} -> type=%s room=%s\n", frame.Type, func() string {{
            if frame.Room == "" {{
                return "-"
            }}
            return frame.Room
        }}())
        ctx.Room = frame.Room
{middleware_calls}{rate_limit_check}        switch frame.Type {{
{ping_builtin}{dispatch}        }}
    }}
{exit_call}
    fmt.Fprintf(os.Stderr, "[{service}] WS disconnect {path} -> CLOSED\n")
    ctx.LeaveAllRooms()
}}

{middleware_defs}
{rate_limit_defs}
"#,
        ctx_name = ctx_name,
        handler_name = handler_name,
        outbound_methods = outbound_methods,
        auth_check = auth_check,
        header_check = header_check,
        middleware_calls = middleware_calls,
        rate_limit_check = rate_limit_check,
        dispatch = dispatch,
        join_call = join_call,
        exit_call = exit_call,
        ping_builtin = ping_builtin,
        middleware_defs = middleware_defs,
        rate_limit_defs = rate_limit_defs,
        room_helpers = room_helpers,
        room_allowed = room_allowed,
        service = service.name,
        path = socket.path
    )
}

fn render_socket_call(call: &CallSpec) -> String {
    let module = &call.module;
    let func = &call.function;
    if call.is_async {
        format!("go func() {{ {module}.{func}(ctx, frame.Data) }}()")
    } else {
        format!("{module}.{func}(ctx, frame.Data)")
    }
}

fn render_socket_auth_check(socket: &SocketDef) -> String {
    let Some(auth) = socket.auth.as_ref() else {
        return String::new();
    };
    match auth {
        crate::ir::AuthSpec::Bearer => {
            "    if r.Header.Get(\"authorization\") == \"\" {\n        _ = ctx.SendError(\"missing authorization\")\n        return\n    }\n".to_string()
        }
        crate::ir::AuthSpec::ApiKey => {
            "    if r.Header.Get(\"x-api-key\") == \"\" {\n        _ = ctx.SendError(\"missing api key\")\n        return\n    }\n".to_string()
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
            "    if r.Header.Get(\"{key}\") == \"\" {{\n        _ = ctx.SendError(\"missing header {name}\")\n        return\n    }}\n",
            key = key,
            name = field.name
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
            "        if err := ws_{name}(ctx, frame.Type); err != nil {{\n            _ = ctx.SendError(err.Error())\n            continue\n        }}\n",
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
            "func ws_{name}(_ctx *{ctx}, message string) error {{\n    kind := strings.TrimSpace(message)\n    if kind == \"\" {{\n        return fmt.Errorf(\"empty socket message kind\")\n    }}\n    if len(kind) > 128 {{\n        return fmt.Errorf(\"socket message kind is too long\")\n    }}\n    return nil\n}}\n\n",
            name = name,
            ctx = ctx_name
        ));
    }
    out
}

fn render_socket_rate_limit_defs(socket: &SocketDef) -> String {
    let Some(limit) = socket.rate_limit.as_ref() else {
        return String::new();
    };
    format!(
        "var rateLimit{code} = rateLimit{{max: {max}, window: time.Duration({window}) * time.Second}}\n\n",
        code = socket.name.code_name(),
        max = limit.max,
        window = limit.per_seconds
    )
}

fn render_socket_rate_limit_check(socket: &SocketDef) -> String {
    if socket.rate_limit.is_none() {
        return String::new();
    }
    format!(
        "        if !rateLimit{code}.Allow() {{\n            _ = ctx.SendError(\"rate limited\")\n            continue\n        }}\n",
        code = socket.name.code_name()
    )
}
