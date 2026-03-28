use anyhow::{Result, anyhow, bail};

pub(super) fn count_leading_spaces(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

pub(super) fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(super) fn is_rust_path(path: &str) -> bool {
    path.split("::").all(is_ident)
}

pub(super) fn parse_quoted_value(raw: &str, line_no: usize) -> Result<String> {
    let raw = raw.trim();
    if !raw.starts_with('"') || !raw.ends_with('"') {
        bail!("Line {}: value must be in double quotes", line_no);
    }
    Ok(raw[1..raw.len() - 1].to_string())
}

pub(super) fn split_alias(input: &str) -> Option<(&str, &str)> {
    let marker = " as ";
    let idx = input.find(marker)?;
    let (left, right) = input.split_at(idx);
    let right = &right[marker.len()..];
    Some((left.trim(), right.trim()))
}

pub(super) fn split_name_path(input: &str) -> Option<(&str, &str)> {
    let mut parts = input.splitn(2, ' ');
    let name = parts.next()?.trim();
    let rest = parts.next()?.trim();
    Some((name, rest))
}

pub(super) fn split_top_level_commas(raw: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth_angle = 0i32;
    let mut depth_brace = 0i32;
    for ch in raw.chars() {
        match ch {
            '<' => depth_angle += 1,
            '>' => depth_angle -= 1,
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            ',' if depth_angle == 0 && depth_brace == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(ch);
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

pub(super) fn extract_braced(raw: &str) -> Result<&str> {
    let raw = raw.trim();
    if !raw.starts_with('{') || !raw.ends_with('}') {
        bail!("Invalid object literal");
    }
    Ok(&raw[1..raw.len() - 1])
}

pub(super) fn normalize_multiline_objects(input: &str) -> Result<String> {
    let mut out = String::new();
    let mut in_object = false;
    let mut buffer = String::new();
    let mut brace_depth = 0i32;

    for line in input.lines() {
        if !in_object {
            if line.contains('{') && !line.contains('}') {
                in_object = true;
                brace_depth = line.chars().filter(|c| *c == '{').count() as i32
                    - line.chars().filter(|c| *c == '}').count() as i32;
                buffer.clear();
                buffer.push_str(line);
                buffer.push(' ');
                continue;
            } else {
                out.push_str(line);
                out.push('\n');
                continue;
            }
        } else {
            brace_depth += line.chars().filter(|c| *c == '{').count() as i32;
            brace_depth -= line.chars().filter(|c| *c == '}').count() as i32;
            let trimmed = line.trim();
            if trimmed == "}" {
                buffer.push_str("}");
            } else {
                buffer.push_str(trimmed);
                if !trimmed.ends_with(',') {
                    buffer.push(',');
                }
                buffer.push(' ');
            }
            if brace_depth <= 0 {
                in_object = false;
                out.push_str(buffer.trim_end());
                out.push('\n');
            }
        }
    }
    if in_object {
        return Err(anyhow!("Unclosed object block"));
    }
    Ok(out)
}
