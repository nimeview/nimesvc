use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::ir::{Service, effective_auth};

use super::util::{auth_middleware_name, needs_middleware};

pub(super) fn generate_middleware(service: &Service, src_dir: &Path) -> Result<()> {
    if !needs_middleware(service) {
        return Ok(());
    }
    let mut names = std::collections::BTreeSet::new();
    for name in &service.common.middleware {
        names.insert(name.clone());
    }
    for route in &service.http.routes {
        for name in &route.middleware {
            names.insert(name.clone());
        }
        if let Some(auth) = effective_auth(route.auth.as_ref(), service.common.auth.as_ref()) {
            names.insert(auth_middleware_name(auth));
        }
    }
    let mut out = String::new();
    out.push_str("import { Request, Response, NextFunction } from 'express';\n\n");
    out.push_str(
        "function middlewareDirective(req: Request, name: string): string | undefined {\n",
    );
    out.push_str("  return req.header(`x-nimesvc-middleware-${name.replace(/_/g, '-')}`)?.trim().toLowerCase();\n");
    out.push_str("}\n\n");
    for name in names {
        let body = match name.as_str() {
            "auth_none" => "  return next();\n".to_string(),
            "auth_bearer" => "  if (!req.header('authorization')) {\n    return res.status(401).json({ error: 'missing authorization' });\n  }\n  return next();\n".to_string(),
            "auth_api_key" => "  if (!req.header('x-api-key')) {\n    return res.status(401).json({ error: 'missing api key' });\n  }\n  return next();\n".to_string(),
            _ => format!(
                "  const directive = middlewareDirective(req, '{name}');\n  if (directive === 'block' || directive === 'deny' || directive === 'forbid') {{\n    return res.status(403).json({{ error: 'blocked by middleware {name}' }});\n  }}\n  res.locals[`nimesvc_{name}`] = true;\n  res.setHeader('x-nimesvc-middleware-{header}', 'ok');\n  return next();\n",
                name = name,
                header = name.replace('_', "-")
            ),
        };
        out.push_str(&format!(
            "export async function {name}(req: Request, res: Response, next: NextFunction) {{\n{body}}}\n\n",
            name = name,
            body = body
        ));
    }
    fs::write(src_dir.join("middleware.ts"), out)
        .with_context(|| "Failed to write src/middleware.ts")?;
    Ok(())
}
