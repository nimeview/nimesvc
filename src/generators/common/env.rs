use crate::ir::Service;

pub fn render_env_checks_rust(service: &Service) -> String {
    if service.common.env.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for env in &service.common.env {
        if let Some(default) = &env.default {
            out.push_str(&format!(
                "    if env::var(\"{name}\").is_err() {{\n        env::set_var(\"{name}\", \"{default}\");\n    }}\n",
                name = env.name,
                default = default.replace('\\', "\\\\").replace('\"', "\\\"")
            ));
        } else {
            out.push_str(&format!(
                "    if env::var(\"{name}\").is_err() {{\n        eprintln!(\"Missing env: {name}\");\n        return;\n    }}\n",
                name = env.name
            ));
        }
    }
    out
}

pub fn render_env_checks_go(service: &Service) -> String {
    if service.common.env.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for env in &service.common.env {
        if let Some(default) = &env.default {
            out.push_str(&format!(
                "    if os.Getenv(\"{name}\") == \"\" {{\n        os.Setenv(\"{name}\", \"{default}\")\n    }}\n",
                name = env.name,
                default = default.replace('\\', "\\\\").replace('\"', "\\\"")
            ));
        } else {
            out.push_str(&format!(
                "    if os.Getenv(\"{name}\") == \"\" {{\n        fmt.Println(\"Missing env: {name}\")\n        return\n    }}\n",
                name = env.name
            ));
        }
    }
    out
}

pub fn render_env_checks_ts(service: &Service) -> String {
    if service.common.env.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for env in &service.common.env {
        if let Some(default) = &env.default {
            out.push_str(&format!(
                "if (!process.env[\"{name}\"]) {{\n  process.env[\"{name}\"] = \"{default}\";\n}}\n",
                name = env.name,
                default = default.replace('\\', "\\\\").replace('\"', "\\\"")
            ));
        } else {
            out.push_str(&format!(
                "if (!process.env[\"{name}\"]) {{\n  console.error(\"Missing env: {name}\");\n  process.exit(1);\n}}\n",
                name = env.name
            ));
        }
    }
    out.push('\n');
    out
}
