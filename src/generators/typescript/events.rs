use crate::ir::{EventsBroker, Service};

use super::util::ts_client_type;

pub(crate) fn build_events_ts(service: &Service) -> String {
    if service.events.definitions.is_empty() {
        return String::new();
    }
    if matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    ) {
        return build_events_redis_ts(service);
    }
    build_events_memory_ts(service)
}

fn build_events_memory_ts(service: &Service) -> String {
    let mut out = String::new();
    out.push_str("import * as Types from './types';\n\n");
    out.push_str("type Handler<T> = (payload: T) => void;\n\n");
    out.push_str("const handlers: Record<string, Handler<any>[]> = {};\n\n");
    out.push_str(
        "function addHandler<T>(name: string, handler: Handler<T>) {\n  if (!handlers[name]) handlers[name] = [];\n  handlers[name].push(handler as Handler<any>);\n}\n\n",
    );
    out.push_str(
        "function emit<T>(name: string, payload: T) {\n  const list = handlers[name] || [];\n  for (const handler of list) handler(payload);\n}\n\n",
    );

    for ev in &service.events.definitions {
        let name = ev.name.code_name();
        let payload = ts_client_type(&ev.payload);
        out.push_str(&format!(
            "export type {name}Event = {payload};\n",
            name = name,
            payload = payload
        ));
        out.push_str(&format!(
            "export function on{name}(handler: Handler<{name}Event>) {{ addHandler(\"{name}\", handler); }}\n",
            name = name
        ));
        out.push_str(&format!(
            "export function emit{name}(payload: {name}Event) {{ emit(\"{name}\", payload); }}\n\n",
            name = name
        ));
    }

    out
}

fn build_events_redis_ts(service: &Service) -> String {
    let cfg = service.events.config.as_ref().unwrap();
    let service_name = service.name.to_lowercase();
    let stream_prefix = cfg.stream_prefix.as_deref().unwrap_or(&service_name);
    let group = cfg.group.as_deref().unwrap_or(&service_name);
    let consumer_override = cfg.consumer.as_deref().unwrap_or("");
    let url = cfg.url.as_deref().unwrap_or("");

    let mut out = String::new();
    out.push_str("import * as Types from './types';\n");
    out.push_str("import { createClient } from 'redis';\n\n");
    out.push_str("type Handler<T> = (payload: T) => void;\n\n");
    out.push_str("const handlers: Record<string, Handler<any>[]> = {};\n\n");
    out.push_str(
        "function addHandler<T>(name: string, handler: Handler<T>) {\n  if (!handlers[name]) handlers[name] = [];\n  handlers[name].push(handler as Handler<any>);\n}\n\n",
    );
    out.push_str(&format!(
        "const EVENTS_STREAM_PREFIX = \"{}\";\nconst EVENTS_GROUP = \"{}\";\nconst EVENTS_CONSUMER_OVERRIDE = \"{}\";\nconst EVENTS_REDIS_URL = \"{}\";\nconst EVENTS_CONSUMER_PREFIX = \"{}\";\n\n",
        stream_prefix, group, consumer_override, url, service_name
    ));
    out.push_str(
        "function redisUrl(): string {\n  if (EVENTS_REDIS_URL) return EVENTS_REDIS_URL;\n  return process.env.REDIS_URL || 'redis://127.0.0.1:6379';\n}\n\n",
    );
    out.push_str(
        "function streamName(event: string): string {\n  return `${EVENTS_STREAM_PREFIX}.${event}`;\n}\n\n",
    );
    out.push_str("function groupName(): string { return EVENTS_GROUP; }\n");
    out.push_str(
        "function consumerName(): string {\n  if (EVENTS_CONSUMER_OVERRIDE) return EVENTS_CONSUMER_OVERRIDE;\n  return `${EVENTS_CONSUMER_PREFIX}-${process.pid}`;\n}\n\n",
    );
    out.push_str(
        "let redisClient: ReturnType<typeof createClient> | null = null;\nasync function getRedis() {\n  if (!redisClient) {\n    redisClient = createClient({ url: redisUrl() });\n    redisClient.on('error', (err: any) => console.error('redis error', err));\n    await redisClient.connect();\n  }\n  return redisClient;\n}\n\n",
    );
    out.push_str(
        "async function publishEvent(stream: string, payload: string) {\n  const client = await getRedis();\n  await client.xAdd(stream, '*', { payload });\n}\n\n",
    );
    out.push_str(
        "async function ensureGroup(stream: string) {\n  const client = await getRedis();\n  try {\n    await client.sendCommand(['XGROUP', 'CREATE', stream, groupName(), '$', 'MKSTREAM']);\n  } catch (err: any) {\n    if (!String(err?.message || err).includes('BUSYGROUP')) {\n      console.error('events: group create error', err);\n    }\n  }\n}\n\n",
    );
    out.push_str(
        "async function consumeLoop<T>(stream: string, handlerList: () => Handler<T>[]) {\n  await ensureGroup(stream);\n  const client = await getRedis();\n  const group = groupName();\n  const consumer = consumerName();\n  while (true) {\n    let reply: any;\n    try {\n      reply = await client.sendCommand(['XREADGROUP', 'GROUP', group, consumer, 'BLOCK', '1000', 'COUNT', '10', 'STREAMS', stream, '>']);\n    } catch (err) {\n      await new Promise((r) => setTimeout(r, 500));\n      continue;\n    }\n    if (!reply) continue;\n    for (const streamBlock of reply) {\n      const entries = streamBlock[1] || [];\n      for (const entry of entries) {\n        const id = entry[0];\n        const fields = entry[1] || [];\n        let payload: any = undefined;\n        for (let i = 0; i < fields.length; i += 2) {\n          if (fields[i] === 'payload') payload = fields[i + 1];\n        }\n        if (payload !== undefined) {\n          try {\n            const parsed = JSON.parse(String(payload));\n            for (const handler of handlerList()) handler(parsed as T);\n          } catch (err) {\n            // ignore parse errors\n          }\n        }\n        try { await client.sendCommand(['XACK', stream, group, id]); } catch (_) {}\n      }\n    }\n  }\n}\n\n",
    );

    for ev in &service.events.definitions {
        let name = ev.name.code_name();
        let payload = ts_client_type(&ev.payload);
        out.push_str(&format!(
            "export type {name}Event = {payload};\n",
            name = name,
            payload = payload
        ));
        out.push_str(&format!(
            "export function on{name}(handler: Handler<{name}Event>) {{ addHandler(\"{name}\", handler); }}\n",
            name = name
        ));
        out.push_str(&format!(
            "export async function emit{name}(payload: {name}Event) {{\n  try {{\n    await publishEvent(streamName(\"{name}\"), JSON.stringify(payload));\n  }} catch (err) {{\n    console.error('events: publish error', err);\n  }}\n}}\n\n",
            name = name
        ));
    }

    if !service.events.subscribes.is_empty() {
        out.push_str("export function startEventConsumers() {\n");
        out.push_str(
            "  const getList = (name: string) => (handlers[name] || []) as Handler<any>[];\n",
        );
        for ev in &service.events.subscribes {
            let name = ev.code_name();
            out.push_str(&format!(
                "  consumeLoop(streamName(\"{name}\"), () => getList(\"{name}\"));\n",
                name = name
            ));
        }
        out.push_str("}\n");
    }

    out
}
