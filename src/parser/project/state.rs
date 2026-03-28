use crate::ir::{AuthSpec, ModuleUse, Service};

use super::super::model::{
    EnumBuilder, EventBuilder, RouteBuilder, RpcBuilder, SocketBuilder, TypeBuilder,
};

#[derive(Default)]
pub(super) struct ProjectState {
    pub(super) current_service: Option<super::super::service::ServiceBuilder>,
    pub(super) current_route: Option<RouteBuilder>,
    pub(super) current_type: Option<TypeBuilder>,
    pub(super) current_enum: Option<EnumBuilder>,
    pub(super) current_event: Option<EventBuilder>,
    pub(super) current_rpc: Option<RpcBuilder>,
    pub(super) current_socket: Option<SocketBuilder>,
    pub(super) current_trigger: Option<super::super::model::SocketTriggerBuilder>,
    pub(super) output: Option<String>,
    pub(super) modules: Vec<ModuleUse>,
    pub(super) auth: Option<AuthSpec>,
    pub(super) middleware: Vec<String>,
    pub(super) default_version: Option<u32>,
    pub(super) global_types: Vec<crate::ir::TypeDef>,
    pub(super) global_enums: Vec<crate::ir::EnumDef>,
    pub(super) global_events: Vec<crate::ir::EventDef>,
    pub(super) global_rpcs: Vec<crate::ir::RpcDef>,
    pub(super) services: Vec<Service>,
}

impl ProjectState {
    pub(super) fn new() -> Self {
        Self::default()
    }
}
