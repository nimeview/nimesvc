use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub(crate) fn write_openapi(
    openapi: &nimesvc::openapi::OpenApi,
    out_path: &PathBuf,
    json: bool,
) -> Result<()> {
    if json {
        let payload = serde_json::to_string_pretty(openapi)?;
        fs::write(out_path, payload)
            .with_context(|| format!("Failed to write '{}'", out_path.display()))?;
    } else {
        let payload = serde_yaml::to_string(openapi)?;
        fs::write(out_path, payload)
            .with_context(|| format!("Failed to write '{}'", out_path.display()))?;
    }
    Ok(())
}

pub(super) fn normalize_module_rel_path(raw: &str) -> Result<PathBuf> {
    let path = Path::new(raw);
    if path.is_absolute() {
        let name = path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid module path '{}'", raw))?;
        return Ok(PathBuf::from(name));
    }
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                anyhow::bail!("Module path '{}' cannot contain '..'", raw)
            }
            std::path::Component::Normal(seg) => out.push(seg),
            _ => {}
        }
    }
    if out.as_os_str().is_empty() {
        anyhow::bail!("Invalid module path '{}'", raw);
    }
    Ok(out)
}

pub(super) fn path_to_import_string(path: &Path) -> String {
    let mut parts = Vec::new();
    for comp in path.components() {
        if let std::path::Component::Normal(seg) = comp {
            parts.push(seg.to_string_lossy().to_string());
        }
    }
    parts.join("/")
}

pub(super) fn stamp_matches(path: &PathBuf, expected: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read '{}'", path.display()))?;
    Ok(content.trim() == expected)
}

pub(super) fn file_hash_hex(path: &PathBuf) -> Result<String> {
    use sha2::{Digest, Sha256};
    let data = fs::read(path).with_context(|| format!("Failed to read '{}'", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}
