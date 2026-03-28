use serde_json::{json, Value};

pub async fn send_message(ctx: ChatSocketContext, data: Value) {
    let room = ctx.room().unwrap_or_else(|| "chat".to_string());
    ctx.join_room(&room);
    let text = data.get("text").and_then(|v| v.as_str()).unwrap_or("");
    ctx.send_room(&room, "MessageOut", json!({"text": text}));
}

pub async fn message_out(_ctx: ChatSocketContext, _data: Value) {
    // TODO: outbound hook if needed
}
