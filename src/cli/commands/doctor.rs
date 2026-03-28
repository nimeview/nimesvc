use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use nimesvc::parser::parse_project;

use super::super::domains::process;

pub(super) fn doctor_cmd() -> Result<()> {
    let mut missing = Vec::new();

    if !check_cmd("cargo", &["--version"]) {
        missing.push("cargo (Rust)");
    }
    if !check_cmd("node", &["--version"]) {
        missing.push("node (TypeScript runtime)");
    }
    if !check_cmd("bun", &["--version"]) {
        missing.push("bun (TypeScript deps/runtime)");
    }
    if process::resolve_go_binary().is_err() {
        missing.push("go (Golang)");
    }

    if !missing.is_empty() {
        println!("Doctor: warnings (missing tools):");
        for item in &missing {
            println!(" - {}", item);
        }
    }

    let entries = find_ns_entries()?;
    if entries.is_empty() {
        println!("Doctor: no .ns files found in current project");
    } else {
        for ns_path in entries {
            validate_modules(&ns_path)?;
        }
    }

    println!("Doctor: OK");
    Ok(())
}

fn find_ns_entries() -> Result<Vec<PathBuf>> {
    let cwd = std::env::current_dir().with_context(|| "Failed to resolve current directory")?;
    let mut entries = Vec::new();
    collect_ns_entries(&cwd, &mut entries)?;
    entries.sort();
    Ok(entries)
}

fn collect_ns_entries(dir: &Path, entries: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory '{}'", dir.display()))?
    {
        let entry = entry.with_context(|| "Failed to read directory entry")?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to read file type for '{}'", path.display()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if file_type.is_dir() {
            if matches!(
                name.as_ref(),
                ".git" | "target" | "node_modules" | ".nimesvc" | "release" | "examples"
            ) {
                continue;
            }
            collect_ns_entries(&path, entries)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("ns") {
            entries.push(path);
        }
    }
    Ok(())
}

fn validate_modules(ns_path: &Path) -> Result<()> {
    let src = fs::read_to_string(ns_path)
        .with_context(|| format!("Failed to read input file '{}'", ns_path.display()))?;
    let project = parse_project(&src)?;
    let base_dir = ns_path.parent().unwrap_or_else(|| Path::new("."));

    let mut errors = Vec::new();
    for service in &project.services {
        let lang = service.language.clone();
        let mut module_sources: std::collections::HashMap<String, (PathBuf, String)> =
            std::collections::HashMap::new();
        for module in project
            .common
            .modules
            .iter()
            .chain(service.common.modules.iter())
        {
            let Some(path) = module.path.as_deref() else {
                continue;
            };
            let module_path = base_dir.join(path);
            if !module_path.exists() {
                errors.push(format!(
                    "Service '{}' module '{}' not found at {}",
                    service.name,
                    module.alias.as_deref().unwrap_or(&module.name),
                    module_path.display()
                ));
                continue;
            }
            let content = fs::read_to_string(&module_path)
                .with_context(|| format!("Failed to read module '{}'", module_path.display()))?;
            if let Some(lang) = lang.as_ref() {
                let ok = match lang {
                    nimesvc::ir::Lang::Rust => {
                        module_path.extension().and_then(|s| s.to_str()) == Some("rs")
                    }
                    nimesvc::ir::Lang::TypeScript => {
                        module_path.extension().and_then(|s| s.to_str()) == Some("ts")
                    }
                    nimesvc::ir::Lang::Go => {
                        module_path.extension().and_then(|s| s.to_str()) == Some("go")
                    }
                };
                if !ok {
                    errors.push(format!(
                        "Service '{}' module '{}' has incompatible extension for {:?}: {}",
                        service.name,
                        module.alias.as_deref().unwrap_or(&module.name),
                        lang,
                        module_path.display()
                    ));
                }
            }
            let key = module.alias.as_deref().unwrap_or(&module.name).to_string();
            module_sources.insert(key, (module_path, content));
        }

        let mut required: std::collections::HashMap<String, std::collections::BTreeSet<String>> =
            std::collections::HashMap::new();
        for route in &service.http.routes {
            if route.call.module.is_empty() || route.call.service_base.is_some() {
                continue;
            }
            required
                .entry(route.call.module.clone())
                .or_default()
                .insert(route.call.function.clone());
        }
        for rpc in &service.rpc.methods {
            if rpc.call.module.is_empty() {
                continue;
            }
            required
                .entry(rpc.call.module.clone())
                .or_default()
                .insert(rpc.call.function.clone());
        }
        for socket in &service.sockets.sockets {
            for msg in socket.inbound.iter().chain(socket.outbound.iter()) {
                if msg.handler.module.is_empty() {
                    continue;
                }
                required
                    .entry(msg.handler.module.clone())
                    .or_default()
                    .insert(msg.handler.function.clone());
            }
        }

        for (module_name, funcs) in required {
            let Some((module_path, content)) = module_sources.get(&module_name) else {
                continue;
            };
            let module_lang =
                lang.clone()
                    .or_else(|| match module_path.extension().and_then(|s| s.to_str()) {
                        Some("rs") => Some(nimesvc::ir::Lang::Rust),
                        Some("ts") => Some(nimesvc::ir::Lang::TypeScript),
                        Some("go") => Some(nimesvc::ir::Lang::Go),
                        _ => None,
                    });
            let Some(module_lang) = module_lang else {
                continue;
            };

            let tokens = tokenize_source(content);
            for func in funcs {
                let exists = match module_lang {
                    nimesvc::ir::Lang::Rust => rust_has_fn(&tokens, &func),
                    nimesvc::ir::Lang::TypeScript => ts_has_fn(&tokens, &func),
                    nimesvc::ir::Lang::Go => go_has_fn(&tokens, &func),
                };
                if !exists {
                    errors.push(format!(
                        "Service '{}' module '{}' missing function '{}' in {}",
                        service.name,
                        module_name,
                        func,
                        module_path.display()
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        return Ok(());
    }

    println!("Doctor: module errors:");
    for err in &errors {
        println!(" - {}", err);
    }
    anyhow::bail!("Doctor failed");
}

fn tokenize_source(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let flush = |out: &mut Vec<String>, buf: &mut String| {
        if !buf.is_empty() {
            out.push(buf.clone());
            buf.clear();
        }
    };
    for ch in src.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            buf.push(ch);
        } else {
            flush(&mut out, &mut buf);
            if "(){}[]=;.".contains(ch) {
                out.push(ch.to_string());
            }
        }
    }
    flush(&mut out, &mut buf);
    out
}

fn rust_has_fn(tokens: &[String], name: &str) -> bool {
    let mut it = tokens.iter();
    while let Some(tok) = it.next() {
        if tok == "fn" {
            if let Some(next) = it.next() {
                if next == name {
                    return true;
                }
            }
        }
    }
    false
}

fn ts_has_fn(tokens: &[String], name: &str) -> bool {
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "function" => {
                if tokens.get(i + 1).map(|t| t.as_str()) == Some(name) {
                    return true;
                }
            }
            "const" | "let" | "var" => {
                if tokens.get(i + 1).map(|t| t.as_str()) == Some(name)
                    && tokens.get(i + 2).map(|t| t.as_str()) == Some("=")
                {
                    return true;
                }
            }
            "export" => {
                if tokens.get(i + 1).map(|t| t.as_str()) == Some("function")
                    && tokens.get(i + 2).map(|t| t.as_str()) == Some(name)
                {
                    return true;
                }
                if matches!(
                    tokens.get(i + 1).map(|t| t.as_str()),
                    Some("const" | "let" | "var")
                ) && tokens.get(i + 2).map(|t| t.as_str()) == Some(name)
                    && tokens.get(i + 3).map(|t| t.as_str()) == Some("=")
                {
                    return true;
                }
            }
            "async" => {
                if tokens.get(i + 1).map(|t| t.as_str()) == Some("function")
                    && tokens.get(i + 2).map(|t| t.as_str()) == Some(name)
                {
                    return true;
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

fn go_has_fn(tokens: &[String], name: &str) -> bool {
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "func" {
            if tokens.get(i + 1).map(|t| t.as_str()) == Some("(") {
                let mut j = i + 2;
                while j < tokens.len() && tokens[j] != ")" {
                    j += 1;
                }
                if tokens.get(j + 1).map(|t| t.as_str()) == Some(name) {
                    return true;
                }
            } else if tokens.get(i + 1).map(|t| t.as_str()) == Some(name) {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn check_cmd(cmd: &str, args: &[&str]) -> bool {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{collect_ns_entries, find_ns_entries};
    use std::fs;

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("nimesvc_doctor_{}_{}", prefix, stamp));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn collect_ns_entries_skips_examples_and_target() {
        let root = temp_dir("collect");
        fs::write(root.join("api.ns"), "service Api:\n").unwrap();
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested").join("worker.ns"), "service Worker:\n").unwrap();
        fs::create_dir_all(root.join("examples")).unwrap();
        fs::write(
            root.join("examples").join("sample.ns"),
            "service Example:\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target").join("ignored.ns"), "service Ignored:\n").unwrap();

        let mut entries = Vec::new();
        collect_ns_entries(&root, &mut entries).unwrap();
        entries.sort();

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|p| p.ends_with("api.ns")));
        assert!(entries.iter().any(|p| p.ends_with("worker.ns")));
    }

    #[test]
    fn find_ns_entries_reads_from_current_directory() {
        let root = temp_dir("find");
        fs::write(root.join("main.ns"), "service Main:\n").unwrap();

        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let entries = find_ns_entries().unwrap();
        std::env::set_current_dir(old).unwrap();

        assert_eq!(entries.len(), 1);
        assert!(entries[0].ends_with("main.ns"));
    }
}
