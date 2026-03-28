use crate::ir::{EventsBroker, Service};

use super::util::{rust_type, to_snake_case};

pub(crate) fn build_events_rs(service: &Service) -> String {
    if service.events.definitions.is_empty() {
        return String::new();
    }
    if matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    ) {
        return build_events_redis_rs(service);
    }
    build_events_memory_rs(service)
}

fn build_events_memory_rs(service: &Service) -> String {
    let mut out = String::new();
    out.push_str("#![allow(dead_code)]\n");
    out.push_str("use once_cell::sync::Lazy;\n");
    out.push_str("use std::sync::{Arc, Mutex};\n\n");

    for ev in &service.events.definitions {
        let name = ev.name.code_name();
        let handler_name = format!("{}_HANDLERS", name.to_uppercase());
        let payload_ty = rust_type(&ev.payload);
        let snake = to_snake_case(&name);
        out.push_str(&format!(
            "static {handler_name}: Lazy<Mutex<Vec<Arc<dyn Fn(&{payload_ty}) + Send + Sync>>>> = Lazy::new(|| Mutex::new(Vec::new()));\n",
            handler_name = handler_name,
            payload_ty = payload_ty
        ));
        out.push_str(&format!(
            "pub fn on_{snake}(handler: impl Fn(&{payload_ty}) + Send + Sync + 'static) {{\n    {handler_name}.lock().unwrap().push(Arc::new(handler));\n}}\n",
            snake = snake,
            payload_ty = payload_ty,
            handler_name = handler_name
        ));
        out.push_str(&format!(
            "pub fn emit_{snake}(payload: &{payload_ty}) {{\n    let handlers = {handler_name}.lock().unwrap().clone();\n    for handler in handlers {{\n        handler(payload);\n    }}\n}}\n\n",
            snake = snake,
            payload_ty = payload_ty,
            handler_name = handler_name
        ));
    }

    out
}

fn build_events_redis_rs(service: &Service) -> String {
    let cfg = service.events.config.as_ref().unwrap();
    let service_name = service.name.to_lowercase();
    let stream_prefix = cfg.stream_prefix.as_deref().unwrap_or(&service_name);
    let group = cfg.group.as_deref().unwrap_or(&service_name);
    let consumer_override = cfg.consumer.as_deref().unwrap_or("");
    let url = cfg.url.as_deref().unwrap_or("");

    let mut out = String::new();
    out.push_str("#![allow(dead_code)]\n");
    out.push_str("use once_cell::sync::Lazy;\n");
    out.push_str("use std::sync::{Arc, Mutex};\n");
    out.push_str("use std::time::Duration;\n");
    out.push_str("use redis::AsyncCommands;\n");
    out.push_str("use serde_json::Value as JsonValue;\n\n");

    out.push_str(&format!(
        "const EVENTS_STREAM_PREFIX: &str = \"{}\";\n",
        stream_prefix
    ));
    out.push_str(&format!("const EVENTS_GROUP: &str = \"{}\";\n", group));
    out.push_str(&format!(
        "const EVENTS_CONSUMER_OVERRIDE: &str = \"{}\";\n",
        consumer_override
    ));
    out.push_str(&format!("const EVENTS_REDIS_URL: &str = \"{}\";\n\n", url));
    out.push_str(&format!(
        "const EVENTS_CONSUMER_PREFIX: &str = \"{}\";\n\n",
        service_name
    ));

    out.push_str(
        r#"
fn redis_url() -> String {
    if !EVENTS_REDIS_URL.is_empty() {
        return EVENTS_REDIS_URL.to_string();
    }
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string())
}

fn stream_name(event: &str) -> String {
    format!("{}.{}", EVENTS_STREAM_PREFIX, event)
}

fn group_name() -> String {
    EVENTS_GROUP.to_string()
}

fn consumer_name() -> String {
    if !EVENTS_CONSUMER_OVERRIDE.is_empty() {
        return EVENTS_CONSUMER_OVERRIDE.to_string();
    }
    format!("{}-{}", EVENTS_CONSUMER_PREFIX, std::process::id())
}

async fn publish_event(stream: String, payload: String) -> redis::RedisResult<()> {
    let client = redis::Client::open(redis_url())?;
    let mut conn = client.get_async_connection().await?;
    let _: () = redis::cmd("XADD")
        .arg(stream)
        .arg("*")
        .arg("payload")
        .arg(payload)
        .query_async(&mut conn)
        .await?;
    Ok(())
}

fn parse_entries(value: redis::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let streams = match value {
        redis::Value::Bulk(v) => v,
        _ => return out,
    };
    for stream in streams {
        let redis::Value::Bulk(stream_parts) = stream else { continue; };
        if stream_parts.len() < 2 {
            continue;
        }
        let redis::Value::Bulk(entries) = &stream_parts[1] else { continue; };
        for entry in entries {
            let redis::Value::Bulk(fields) = entry else { continue; };
            if fields.len() < 2 {
                continue;
            }
            let id = match &fields[0] {
                redis::Value::Data(d) => String::from_utf8_lossy(d).to_string(),
                _ => continue,
            };
            let redis::Value::Bulk(kv) = &fields[1] else { continue; };
            let mut payload = None;
            let mut it = kv.iter();
            while let (Some(k), Some(v)) = (it.next(), it.next()) {
                let key = match k {
                    redis::Value::Data(d) => String::from_utf8_lossy(d).to_string(),
                    _ => continue,
                };
                if key == "payload" {
                    if let redis::Value::Data(d) = v {
                        payload = Some(String::from_utf8_lossy(d).to_string());
                    }
                }
            }
            if let Some(payload) = payload {
                out.push((id, payload));
            }
        }
    }
    out
}

fn handler_snapshot<T>(
    handlers: &'static Lazy<Mutex<Vec<Arc<dyn Fn(&T) + Send + Sync>>>>,
) -> Vec<Arc<dyn Fn(&T) + Send + Sync>> {
    handlers.lock().unwrap().clone()
}

async fn consume_loop<T: serde::de::DeserializeOwned + Send + 'static>(
    stream: String,
    handlers: &'static Lazy<Mutex<Vec<Arc<dyn Fn(&T) + Send + Sync>>>>,
) {
    let group = group_name();
    let consumer = consumer_name();
    let client = match redis::Client::open(redis_url()) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("events: redis error {}", err);
            return;
        }
    };
    let mut conn = match client.get_async_connection().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("events: redis connection error {}", err);
            return;
        }
    };
    let _ : redis::RedisResult<()> = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(&stream)
        .arg(&group)
        .arg("$")
        .arg("MKSTREAM")
        .query_async(&mut conn)
        .await;

    loop {
        let reply: redis::RedisResult<redis::Value> = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&group)
            .arg(&consumer)
            .arg("BLOCK")
            .arg(1000)
            .arg("COUNT")
            .arg(10)
            .arg("STREAMS")
            .arg(&stream)
            .arg(">")
            .query_async(&mut conn)
            .await;
        let Ok(value) = reply else {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        };
        let entries = parse_entries(value);
        for (id, payload) in entries {
            if let Ok(parsed) = serde_json::from_str::<T>(&payload) {
                for handler in handler_snapshot(handlers) {
                    handler(&parsed);
                }
            }
            let _ : redis::RedisResult<()> = redis::cmd("XACK")
                .arg(&stream)
                .arg(&group)
                .arg(&id)
                .query_async(&mut conn)
                .await;
        }
    }
}
"#,
    );

    for ev in &service.events.definitions {
        let name = ev.name.code_name();
        let handler_name = format!("{}_HANDLERS", name.to_uppercase());
        let payload_ty = rust_type(&ev.payload);
        let snake = to_snake_case(&name);
        out.push_str(&format!(
            "static {handler_name}: Lazy<Mutex<Vec<Arc<dyn Fn(&{payload_ty}) + Send + Sync>>>> = Lazy::new(|| Mutex::new(Vec::new()));\n",
            handler_name = handler_name,
            payload_ty = payload_ty
        ));
        out.push_str(&format!(
            "pub fn on_{snake}(handler: impl Fn(&{payload_ty}) + Send + Sync + 'static) {{\n    {handler_name}.lock().unwrap().push(Arc::new(handler));\n}}\n",
            snake = snake,
            payload_ty = payload_ty,
            handler_name = handler_name
        ));
        out.push_str(&format!(
            "pub fn emit_{snake}(payload: &{payload_ty}) {{\n    let payload = serde_json::to_string(payload).unwrap_or_else(|_| \"{{}}\".to_string());\n    let stream = stream_name(\"{name}\");\n    tokio::spawn(async move {{\n        if let Err(err) = publish_event(stream, payload).await {{\n            eprintln!(\"events: publish error {{}}\", err);\n        }}\n    }});\n}}\n\n",
            snake = snake,
            payload_ty = payload_ty,
            name = name
        ));
    }

    if !service.events.subscribes.is_empty() {
        out.push_str("pub async fn start_event_consumers() {\n");
        for ev in &service.events.subscribes {
            let name = ev.code_name();
            let handler_name = format!("{}_HANDLERS", name.to_uppercase());
            let payload_ty = service
                .events
                .definitions
                .iter()
                .find(|e| e.name.code_name() == name)
                .map(|e| rust_type(&e.payload))
                .unwrap_or_else(|| "JsonValue".to_string());
            out.push_str(&format!(
                "    let stream = stream_name(\"{name}\");\n    tokio::spawn(consume_loop::<{payload_ty}>(stream, &{handler_name}));\n",
                name = name,
                payload_ty = payload_ty,
                handler_name = handler_name
            ));
        }
        out.push_str("}\n");
    }

    out
}
