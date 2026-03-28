use anyhow::{Result, bail};

use crate::ir::{Project, ProjectCommon};

use super::events::validate_event_payloads;
use super::rpc::validate_rpcs;
use super::service::{
    apply_default_version, collect_all_rpcs, collect_known_types, validate_event_refs,
};
use super::types::validate_named_types;
use super::util::{count_leading_spaces, normalize_multiline_objects};

mod handlers;
mod state;

use handlers::{
    finalize_eof, handle_indent0, handle_indent4, handle_indent8, handle_indent12, handle_indent16,
};
use state::ProjectState;

pub fn parse_project(input: &str) -> Result<Project> {
    let input = normalize_multiline_objects(input)?;

    let mut state = ProjectState::new();

    for (idx, raw_line) in input.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim_end();

        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

        if line.contains('\t') {
            bail!("Line {}: tabs are not allowed; use spaces", line_no);
        }

        let indent = count_leading_spaces(line);
        let content = line.trim_start();

        match indent {
            0 => handle_indent0(&mut state, content, line_no)?,
            4 => handle_indent4(&mut state, content, line_no)?,
            8 => handle_indent8(&mut state, content, line_no)?,
            12 => handle_indent12(&mut state, content, line_no)?,
            16 => handle_indent16(&mut state, content, line_no)?,
            _ => {
                bail!(
                    "Line {}: invalid indentation (use 0, 4, 8, 12, or 16 spaces)",
                    line_no
                );
            }
        }
    }

    let eof_line = input.lines().count() + 1;
    finalize_eof(&mut state, eof_line)?;

    if state.services.is_empty() {
        bail!("Missing service declaration");
    }

    let mut project = Project {
        common: ProjectCommon {
            output: state.output,
            modules: state.modules,
            auth: state.auth,
            middleware: state.middleware,
            default_version: state.default_version,
        },
        services: state.services,
    };
    apply_default_version(&mut project)?;
    for service in &project.services {
        validate_named_types(
            &service.schema.types,
            &service.schema.enums,
            &service.http.routes,
            &service.http.headers,
            &service.rpc.methods,
            &service.sockets.sockets,
        )?;
        validate_event_payloads(
            &service.events.definitions,
            &collect_known_types(&service.schema.types, &service.schema.enums),
        )?;
        validate_event_refs(service)?;
    }
    validate_rpcs(&collect_all_rpcs(&project.services), &project.services)?;
    Ok(project)
}
