use crate::generators::common::headers::header_runtime_key;

pub fn missing_header_status(name: &str) -> u16 {
    let key = header_runtime_key(name);
    match key.as_str() {
        "authorization" | "x-api-key" => 401,
        _ => 400,
    }
}
