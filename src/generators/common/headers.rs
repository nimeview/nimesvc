pub fn header_runtime_key(name: &str) -> String {
    name.trim().to_lowercase().replace('_', "-")
}

pub fn header_dsl_key(name: &str) -> String {
    name.trim().to_lowercase().replace('-', "_")
}

pub fn default_cors_methods() -> &'static str {
    "GET,POST,PUT,PATCH,DELETE,OPTIONS"
}

pub fn default_cors_headers() -> &'static str {
    "authorization,content-type,x-admin-token,x_admin_token"
}
