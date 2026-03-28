use anyhow::{Result, anyhow, bail};

use crate::ir::{EventDef, Type};

use super::model::EventBuilder;
use super::types::parse_type_with_validation;

pub(super) fn parse_event_decl(content: &str, line_no: usize) -> Result<EventBuilder> {
    let content = content.trim();
    if !content.starts_with("event ") || !content.ends_with(':') {
        bail!("Line {}: expected `event <Name>:`", line_no);
    }
    let name = content
        .strip_prefix("event ")
        .and_then(|s| s.strip_suffix(':'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Line {}: invalid event name", line_no))?;

    let name = super::types::parse_name_with_version(name, line_no)?;
    Ok(EventBuilder {
        name,
        payload: None,
    })
}

pub(super) fn parse_event_payload_line(
    event: &mut EventBuilder,
    content: &str,
    line_no: usize,
) -> Result<()> {
    let content = content.trim();
    if !content.starts_with("payload") {
        bail!("Line {}: expected `payload <Type>`", line_no);
    }
    let raw = content
        .trim_start_matches("payload")
        .trim_start_matches(':')
        .trim();
    if raw.is_empty() {
        bail!("Line {}: missing payload type", line_no);
    }
    let (ty, _validation) = parse_type_with_validation(raw, line_no)?;
    event.payload = Some(ty);
    Ok(())
}

pub(super) fn finalize_event(
    event: Option<EventBuilder>,
    line_no: usize,
) -> Result<Option<EventDef>> {
    let Some(event) = event else {
        return Ok(None);
    };
    let payload = event.payload.ok_or_else(|| {
        anyhow!(
            "Line {}: event '{}' missing payload",
            line_no,
            event.name.display()
        )
    })?;
    Ok(Some(EventDef {
        name: event.name,
        payload,
    }))
}

pub(super) fn validate_event_payloads(
    events: &[EventDef],
    known: &std::collections::HashSet<crate::ir::TypeName>,
) -> Result<()> {
    for ev in events {
        validate_type_ref(&ev.payload, known, &ev.name.display())?;
    }
    Ok(())
}

fn validate_type_ref(
    ty: &Type,
    known: &std::collections::HashSet<crate::ir::TypeName>,
    ctx: &str,
) -> Result<()> {
    match ty {
        Type::Named(name) => {
            if !known.contains(name) {
                bail!("Unknown type '{}' in event {}", name.display(), ctx);
            }
        }
        Type::Array(inner) => validate_type_ref(inner, known, ctx)?,
        Type::Object(fields) => {
            for field in fields {
                validate_type_ref(&field.ty, known, ctx)?;
            }
        }
        _ => {}
    }
    Ok(())
}
