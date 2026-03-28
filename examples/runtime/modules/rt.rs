use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Once;

use serde_json::{json, Value};

use crate::events::{emit_user_created_v1, on_user_created_v1};
use crate::types::{StatusV1, UserV2};

static COUNT: AtomicI64 = AtomicI64::new(0);
static INIT: Once = Once::new();

fn init_events() {
    INIT.call_once(|| {
        on_user_created_v1(|_payload| {
            COUNT.fetch_add(1, Ordering::SeqCst);
        });
    });
}

pub fn health() -> String {
    "ok".to_string()
}

pub fn create_user(name: String, email: String) -> UserV2 {
    init_events();
    let user = UserV2 {
        id: 1,
        name: name.clone(),
        email,
    };
    emit_user_created_v1(&json!({
        "id": user.id,
        "name": name,
    }));
    user
}

pub fn emit_event() -> String {
    init_events();
    emit_user_created_v1(&json!({
        "id": 99,
        "name": "tester",
    }));
    "emitted".to_string()
}

pub fn get_count() -> i64 {
    COUNT.load(Ordering::SeqCst)
}

pub fn get_user(id: i64) -> UserV2 {
    UserV2 {
        id,
        name: "user".to_string(),
        email: "user@example.com".to_string(),
    }
}

pub async fn ws_join(ctx: ChatV1SocketContext, _data: Value) {
    ctx.send_raw("MessageOut", json!({ "text": "joined" }));
}

pub async fn ws_exit(_ctx: ChatV1SocketContext, _data: Value) {
    // TODO: cleanup
}

pub async fn ws_message_in(ctx: ChatV1SocketContext, data: Value) {
    ctx.send_raw("MessageOut", data);
}

pub async fn ws_ping(_ctx: ChatV1SocketContext, _data: Value) {
    // Pong is automatic
}

pub async fn ws_message_out(_ctx: ChatV1SocketContext, _data: Value) {
    // hook
}

pub async fn ws_error(_ctx: ChatV1SocketContext, _data: Value) {
    // hook
}
