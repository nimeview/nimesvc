use anyhow::Result;

pub(crate) fn apply_default_version(project: &mut crate::ir::Project) -> Result<()> {
    let Some(default_version) = project.common.default_version else {
        return Ok(());
    };
    for service in &mut project.services {
        for ty in &mut service.schema.types {
            if ty.name.version.is_none() {
                ty.name.version = Some(default_version);
            }
            for field in &mut ty.fields {
                apply_default_version_to_type(&mut field.ty, default_version);
            }
        }
        for en in &mut service.schema.enums {
            if en.name.version.is_none() {
                en.name.version = Some(default_version);
            }
        }
        for ev in &mut service.events.definitions {
            if ev.name.version.is_none() {
                ev.name.version = Some(default_version);
            }
            apply_default_version_to_type(&mut ev.payload, default_version);
        }
        for rpc in &mut service.rpc.methods {
            if rpc.version.is_none() {
                rpc.version = Some(default_version);
            }
            for field in &mut rpc.input {
                apply_default_version_to_type(&mut field.ty, default_version);
            }
            for field in &mut rpc.headers {
                apply_default_version_to_type(&mut field.ty, default_version);
            }
            apply_default_version_to_type(&mut rpc.output, default_version);
        }
        for socket in &mut service.sockets.sockets {
            if socket.name.version.is_none() {
                socket.name.version = Some(default_version);
            }
            for msg in socket.inbound.iter_mut().chain(socket.outbound.iter_mut()) {
                if msg.name.version.is_none() {
                    msg.name.version = Some(default_version);
                }
            }
            for trigger in &mut socket.triggers {
                if trigger.name.version.is_none() {
                    trigger.name.version = Some(default_version);
                }
                apply_default_version_to_type(&mut trigger.payload, default_version);
            }
            for field in &mut socket.headers {
                apply_default_version_to_type(&mut field.ty, default_version);
            }
        }
        for ev in &mut service.events.emits {
            if ev.version.is_none() {
                ev.version = Some(default_version);
            }
        }
        for ev in &mut service.events.subscribes {
            if ev.version.is_none() {
                ev.version = Some(default_version);
            }
        }
        for field in &mut service.http.headers {
            apply_default_version_to_type(&mut field.ty, default_version);
        }
        for route in &mut service.http.routes {
            for field in route
                .input
                .path
                .iter_mut()
                .chain(route.input.query.iter_mut())
                .chain(route.input.body.iter_mut())
                .chain(route.headers.iter_mut())
            {
                apply_default_version_to_type(&mut field.ty, default_version);
            }
            for resp in &mut route.responses {
                apply_default_version_to_type(&mut resp.ty, default_version);
            }
        }
    }
    Ok(())
}

fn apply_default_version_to_type(ty: &mut crate::ir::Type, default_version: u32) {
    match ty {
        crate::ir::Type::Named(name) => {
            if name.version.is_none() {
                name.version = Some(default_version);
            }
        }
        crate::ir::Type::Array(inner) => {
            apply_default_version_to_type(inner, default_version);
        }
        crate::ir::Type::Map(inner) => {
            apply_default_version_to_type(inner, default_version);
        }
        crate::ir::Type::Union(items) | crate::ir::Type::OneOf(items) => {
            for item in items {
                apply_default_version_to_type(item, default_version);
            }
        }
        crate::ir::Type::Nullable(inner) => {
            apply_default_version_to_type(inner, default_version);
        }
        crate::ir::Type::Object(fields) => {
            for field in fields {
                apply_default_version_to_type(&mut field.ty, default_version);
            }
        }
        _ => {}
    }
}
