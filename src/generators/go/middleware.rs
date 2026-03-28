use crate::ir::{Service, effective_auth};

use super::util::{auth_middleware_name, middleware_name};

pub(super) fn build_middleware_go(service: &Service) -> String {
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
    if names.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("package main\n\n");
    out.push_str("import (\n    \"net/http\"\n    \"strings\"\n)\n\n");
    out.push_str("func middlewareDirective(r *http.Request, name string) string {\n");
    out.push_str("    key := \"x-nimesvc-middleware-\" + strings.ReplaceAll(name, \"_\", \"-\")\n");
    out.push_str("    return strings.ToLower(strings.TrimSpace(r.Header.Get(key)))\n");
    out.push_str("}\n\n");
    for name in names {
        let fn_name = middleware_name(&name);
        let body = match name.as_str() {
            "auth_none" => "        next.ServeHTTP(w, r)\n".to_string(),
            "auth_bearer" => "        if strings.TrimSpace(r.Header.Get(\"Authorization\")) == \"\" {\n            http.Error(w, \"missing authorization\", http.StatusUnauthorized)\n            return\n        }\n        next.ServeHTTP(w, r)\n".to_string(),
            "auth_api_key" => "        if strings.TrimSpace(r.Header.Get(\"X-Api-Key\")) == \"\" {\n            http.Error(w, \"missing api key\", http.StatusUnauthorized)\n            return\n        }\n        next.ServeHTTP(w, r)\n".to_string(),
            _ => format!(
                "        directive := middlewareDirective(r, {name:?})\n        if directive == \"block\" || directive == \"deny\" || directive == \"forbid\" {{\n            http.Error(w, \"blocked by middleware {name}\", http.StatusForbidden)\n            return\n        }}\n        w.Header().Set(\"x-nimesvc-middleware-{header}\", \"ok\")\n        next.ServeHTTP(w, r)\n",
                name = name,
                header = name.replace('_', "-")
            ),
        };
        out.push_str(&format!(
            "func {name}(next http.Handler) http.Handler {{\n    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {{\n{body}    }})\n}}\n\n",
            name = fn_name,
            body = body
        ));
    }
    out
}
