use anyhow::{Result, bail};

use crate::ir::{
    AuthSpec, ModuleUse, Service, ServiceCommon, ServiceEvents, ServiceHttp, ServiceRpc,
    ServiceSchema, ServiceSockets,
};

use super::super::events::finalize_event;
use super::super::model::{
    EnumBuilder, EventBuilder, RouteBuilder, RpcBuilder, SocketBuilder, TypeBuilder,
};
use super::super::route::finalize_route;
use super::super::rpc::finalize_rpc;
use super::super::socket::{finalize_socket, finalize_trigger};
use super::super::types::{finalize_enum, finalize_type, validate_named_types};

#[derive(Debug, Clone)]
pub(crate) struct ServiceBuilder {
    pub(crate) name: String,
    pub(crate) language: Option<crate::ir::Lang>,
    pub(crate) modules: Vec<ModuleUse>,
    pub(crate) auth: Option<AuthSpec>,
    pub(crate) middleware: Vec<String>,
    pub(crate) address: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) grpc_config: Option<crate::ir::GrpcConfig>,
    pub(crate) events_config: Option<crate::ir::EventsConfig>,
    pub(crate) base_url: Option<String>,
    pub(crate) cors: Option<crate::ir::CorsConfig>,
    pub(crate) rate_limit: Option<crate::ir::RateLimit>,
    pub(crate) env: Vec<crate::ir::EnvVar>,
    pub(crate) headers: Vec<crate::ir::Field>,
    pub(crate) types: Vec<crate::ir::TypeDef>,
    pub(crate) enums: Vec<crate::ir::EnumDef>,
    pub(crate) events: Vec<crate::ir::EventDef>,
    pub(crate) rpcs: Vec<crate::ir::RpcDef>,
    pub(crate) sockets: Vec<crate::ir::SocketDef>,
    pub(crate) emits: Vec<crate::ir::TypeName>,
    pub(crate) subscribes: Vec<crate::ir::TypeName>,
    pub(crate) routes: Vec<crate::ir::Route>,
    pub(crate) block: ServiceBlock,
}

pub(crate) fn finalize_service(
    mut service: Option<ServiceBuilder>,
    current_route: Option<RouteBuilder>,
    current_type: Option<TypeBuilder>,
    current_enum: Option<EnumBuilder>,
    current_event: Option<EventBuilder>,
    current_rpc: Option<RpcBuilder>,
    mut current_socket: Option<SocketBuilder>,
    current_trigger: Option<super::super::model::SocketTriggerBuilder>,
    line_no: usize,
    global_types: &[crate::ir::TypeDef],
    global_enums: &[crate::ir::EnumDef],
    global_events: &[crate::ir::EventDef],
    global_rpcs: &[crate::ir::RpcDef],
) -> Result<Option<Service>> {
    let Some(mut service) = service.take() else {
        return Ok(None);
    };
    if let Some(route) = finalize_route(current_route, line_no)? {
        service.routes.push(route);
    }
    if let Some(td) = finalize_type(current_type, line_no)? {
        service.types.push(td);
    }
    if let Some(ed) = finalize_enum(current_enum, line_no)? {
        service.enums.push(ed);
    }
    if let Some(ev) = finalize_event(current_event, line_no)? {
        service.events.push(ev);
    }
    if let Some(rpc) = finalize_rpc(current_rpc, line_no)? {
        service.rpcs.push(rpc);
    }
    if let Some(trigger) = finalize_trigger(current_trigger, line_no)? {
        if let Some(socket) = current_socket.as_mut() {
            socket.triggers.push(trigger);
        } else {
            bail!("Line {}: trigger defined outside socket", line_no);
        }
    }
    if let Some(socket) = finalize_socket(current_socket, line_no)? {
        service.sockets.push(socket);
    }
    if !global_types.is_empty() {
        service.types.extend(global_types.iter().cloned());
    }
    if !global_enums.is_empty() {
        service.enums.extend(global_enums.iter().cloned());
    }
    if !global_events.is_empty() {
        service.events.extend(global_events.iter().cloned());
    }
    if !global_rpcs.is_empty() {
        service.rpcs.extend(
            global_rpcs
                .iter()
                .filter(|r| r.service == service.name)
                .cloned(),
        );
    }
    if service.routes.is_empty() && service.rpcs.is_empty() && service.sockets.is_empty() {
        bail!("Service '{}' has no routes or rpc methods", service.name);
    }
    if !service.headers.is_empty() {
        for route in &mut service.routes {
            for hdr in &service.headers {
                if !route.headers.iter().any(|h| h.name == hdr.name) {
                    route.headers.push(hdr.clone());
                }
            }
        }
    }
    validate_named_types(
        &service.types,
        &service.enums,
        &service.routes,
        &service.headers,
        &service.rpcs,
        &service.sockets,
    )?;
    Ok(Some(Service {
        name: service.name,
        language: service.language,
        common: ServiceCommon {
            modules: service.modules,
            auth: service.auth,
            middleware: service.middleware,
            address: service.address,
            port: service.port,
            base_url: service.base_url,
            cors: service.cors,
            rate_limit: service.rate_limit,
            env: service.env,
        },
        schema: ServiceSchema {
            types: service.types,
            enums: service.enums,
        },
        http: ServiceHttp {
            headers: service.headers,
            routes: service.routes,
        },
        rpc: ServiceRpc {
            grpc_config: service.grpc_config,
            methods: service.rpcs,
        },
        sockets: ServiceSockets {
            sockets: service.sockets,
        },
        events: ServiceEvents {
            config: service.events_config,
            definitions: service.events,
            emits: service.emits,
            subscribes: service.subscribes,
        },
    }))
}

pub(crate) fn collect_known_types(
    types: &[crate::ir::TypeDef],
    enums: &[crate::ir::EnumDef],
) -> std::collections::HashSet<crate::ir::TypeName> {
    let mut known = std::collections::HashSet::new();
    for ty in types {
        known.insert(ty.name.clone());
    }
    for en in enums {
        known.insert(en.name.clone());
    }
    known
}

pub(crate) fn collect_all_rpcs(services: &[Service]) -> Vec<crate::ir::RpcDef> {
    let mut out = Vec::new();
    for service in services {
        out.extend(service.rpc.methods.iter().cloned());
    }
    out
}

pub(crate) fn validate_event_refs(service: &Service) -> Result<()> {
    let known: std::collections::HashSet<crate::ir::TypeName> = service
        .events
        .definitions
        .iter()
        .map(|e| e.name.clone())
        .collect();
    for ev in service
        .events
        .emits
        .iter()
        .chain(service.events.subscribes.iter())
    {
        if !known.contains(ev) {
            bail!(
                "Unknown event '{}' in service '{}'",
                ev.display(),
                service.name
            );
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServiceBlock {
    None,
    Config,
    GrpcConfig,
    EventsConfig,
    Headers,
}
