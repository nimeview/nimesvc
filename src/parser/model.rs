use crate::ir::{AuthSpec, CallSpec, EnumVariant, HttpMethod, Input, ResponseSpec};

#[derive(Debug, Clone)]
pub(super) struct RouteBuilder {
    pub(super) method: HttpMethod,
    pub(super) path: String,
    pub(super) input: Input,
    pub(super) responses: Vec<ResponseSpec>,
    pub(super) input_mode: InputMode,
    pub(super) auth: Option<AuthSpec>,
    pub(super) middleware: Vec<String>,
    pub(super) call: Option<CallSpec>,
    pub(super) headers: Vec<crate::ir::Field>,
    pub(super) response_mode: bool,
    pub(super) rate_limit: Option<crate::ir::RateLimit>,
    pub(super) healthcheck: bool,
}

#[derive(Debug, Clone)]
pub(super) struct TypeBuilder {
    pub(super) name: crate::ir::TypeName,
    pub(super) fields: Vec<crate::ir::Field>,
    pub(super) sealed: bool,
}

#[derive(Debug, Clone)]
pub(super) struct EnumBuilder {
    pub(super) name: crate::ir::TypeName,
    pub(super) variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub(super) struct EventBuilder {
    pub(super) name: crate::ir::TypeName,
    pub(super) payload: Option<crate::ir::Type>,
}

#[derive(Debug, Clone)]
pub(super) struct RpcBuilder {
    pub(super) service: String,
    pub(super) name: crate::ir::TypeName,
    pub(super) input: Vec<crate::ir::Field>,
    pub(super) headers: Vec<crate::ir::Field>,
    pub(super) output: Option<crate::ir::Type>,
    pub(super) block: super::rpc::RpcBlock,
    pub(super) call: Option<crate::ir::CallSpec>,
    pub(super) modules: Vec<crate::ir::ModuleUse>,
    pub(super) auth: Option<crate::ir::AuthSpec>,
    pub(super) middleware: Vec<String>,
    pub(super) rate_limit: Option<crate::ir::RateLimit>,
}

#[derive(Debug, Clone)]
pub(super) struct SocketBuilder {
    pub(super) name: crate::ir::TypeName,
    pub(super) path: String,
    pub(super) rooms: Vec<String>,
    pub(super) triggers: Vec<crate::ir::SocketTrigger>,
    pub(super) inbound: Vec<crate::ir::SocketMessage>,
    pub(super) outbound: Vec<crate::ir::SocketMessage>,
    pub(super) modules: Vec<crate::ir::ModuleUse>,
    pub(super) auth: Option<crate::ir::AuthSpec>,
    pub(super) middleware: Vec<String>,
    pub(super) rate_limit: Option<crate::ir::RateLimit>,
    pub(super) headers: Vec<crate::ir::Field>,
    pub(super) block: super::socket::SocketBlock,
}

#[derive(Debug, Clone)]
pub(super) struct SocketTriggerBuilder {
    pub(super) name: crate::ir::TypeName,
    pub(super) room: Option<String>,
    pub(super) payload: Option<crate::ir::Type>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputMode {
    None,
    InInput,
    InPath,
    InQuery,
    InBody,
    InHeaders,
}
