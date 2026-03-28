use serde_json::Value;

#[derive(Clone)]
pub struct ChatSocketContext;

impl ChatSocketContext {
    pub fn send_raw(&self, _kind: &str, _data: Value) {}
    pub fn send_error(&self, _message: &str) {}
}

pub async fn join(_ctx: ChatSocketContext, _data: Value) {
    // TODO: handle join
}

pub async fn exit(_ctx: ChatSocketContext, _data: Value) {
    // TODO: handle exit
}

pub async fn message_in(_ctx: ChatSocketContext, _data: Value) {
    // TODO: handle message in
}

pub async fn typing(_ctx: ChatSocketContext, _data: Value) {
    // TODO: handle typing
}

pub async fn ping(_ctx: ChatSocketContext, _data: Value) {
    // TODO: handle ping (Pong is sent automatically)
}

pub async fn message_out(_ctx: ChatSocketContext, _data: Value) {
    // TODO: handle message out hook
}

pub async fn user_joined(_ctx: ChatSocketContext, _data: Value) {
    // TODO: handle user joined
}

pub async fn user_left(_ctx: ChatSocketContext, _data: Value) {
    // TODO: handle user left
}

pub async fn error(_ctx: ChatSocketContext, _data: Value) {
    // TODO: handle error
}

pub async fn notice(_ctx: ChatSocketContext, _data: Value) {
    // TODO: handle server notice
}
