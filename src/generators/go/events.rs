use crate::ir::{EventsBroker, Service};

use super::util::{go_type, type_uses_types_pkg};

pub(crate) fn build_events_go(service: &Service, module_name: &str) -> String {
    if service.events.definitions.is_empty() {
        return String::new();
    }
    if matches!(
        service.events.config.as_ref().map(|c| &c.broker),
        Some(EventsBroker::Redis)
    ) {
        return build_events_redis_go(service, module_name);
    }
    build_events_memory_go(service, module_name)
}

fn build_events_memory_go(service: &Service, module_name: &str) -> String {
    let mut out = String::new();
    out.push_str("package main\n\n");
    let mut imports = vec!["\"sync\"".to_string()];
    let needs_types = service
        .events
        .definitions
        .iter()
        .any(|ev| type_uses_types_pkg(&ev.payload));
    if needs_types {
        imports.push(format!("\"{}/types\"", module_name));
    }
    out.push_str("import (\n");
    out.push_str(
        &imports
            .into_iter()
            .map(|i| format!("    {}", i))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    out.push_str("\n)\n\n");

    for ev in &service.events.definitions {
        let name = ev.name.code_name();
        let payload_ty = go_type(&ev.payload, true);
        let handlers = format!("{}_Handlers", name);
        out.push_str(&format!(
            "var {handlers} = struct {{ sync.Mutex; list []func({payload_ty}) }}{{}}\n",
            handlers = handlers,
            payload_ty = payload_ty
        ));
        out.push_str(&format!(
            "func On{name}(handler func({payload_ty})) {{\n    {handlers}.Lock()\n    defer {handlers}.Unlock()\n    {handlers}.list = append({handlers}.list, handler)\n}}\n",
            name = name,
            payload_ty = payload_ty,
            handlers = handlers
        ));
        out.push_str(&format!(
            "func Emit{name}(payload {payload_ty}) {{\n    {handlers}.Lock()\n    list := append([]func({payload_ty}){{}}, {handlers}.list...)\n    {handlers}.Unlock()\n    for _, handler := range list {{\n        handler(payload)\n    }}\n}}\n\n",
            name = name,
            payload_ty = payload_ty,
            handlers = handlers
        ));
    }

    out
}

fn build_events_redis_go(service: &Service, module_name: &str) -> String {
    let cfg = service.events.config.as_ref().unwrap();
    let service_name = service.name.to_lowercase();
    let stream_prefix = cfg.stream_prefix.as_deref().unwrap_or(&service_name);
    let group = cfg.group.as_deref().unwrap_or(&service_name);
    let consumer_override = cfg.consumer.as_deref().unwrap_or("");
    let url = cfg.url.as_deref().unwrap_or("");

    let mut out = String::new();
    out.push_str("package main\n\n");
    let mut imports = vec![
        "\"context\"".to_string(),
        "\"encoding/json\"".to_string(),
        "\"fmt\"".to_string(),
        "\"os\"".to_string(),
        "\"strings\"".to_string(),
        "\"sync\"".to_string(),
        "\"time\"".to_string(),
        "\"github.com/redis/go-redis/v9\"".to_string(),
    ];
    let needs_types = service
        .events
        .definitions
        .iter()
        .any(|ev| type_uses_types_pkg(&ev.payload));
    if needs_types {
        imports.push(format!("\"{}/types\"", module_name));
    }
    out.push_str("import (\n");
    out.push_str(
        &imports
            .into_iter()
            .map(|i| format!("    {}", i))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    out.push_str("\n)\n\n");

    out.push_str(&format!(
        "const eventsStreamPrefix = \"{}\"\n",
        stream_prefix
    ));
    out.push_str(&format!("const eventsGroup = \"{}\"\n", group));
    out.push_str(&format!(
        "const eventsConsumerOverride = \"{}\"\n",
        consumer_override
    ));
    out.push_str(&format!("const eventsRedisURL = \"{}\"\n", url));
    out.push_str(&format!(
        "const eventsConsumerPrefix = \"{}\"\n\n",
        service_name
    ));

    out.push_str(
        r#"
var redisOnce sync.Once
var redisClient *redis.Client

func redisURL() string {
    if eventsRedisURL != "" {
        return eventsRedisURL
    }
    if env := os.Getenv("REDIS_URL"); env != "" {
        return env
    }
    return "redis://127.0.0.1:6379"
}

func streamName(event string) string {
    return fmt.Sprintf("%s.%s", eventsStreamPrefix, event)
}

func groupName() string {
    return eventsGroup
}

func consumerName() string {
    if eventsConsumerOverride != "" {
        return eventsConsumerOverride
    }
    return fmt.Sprintf("%s-%d", eventsConsumerPrefix, os.Getpid())
}

func getRedis() *redis.Client {
    redisOnce.Do(func() {
        url := redisURL()
        if strings.HasPrefix(url, "redis://") || strings.HasPrefix(url, "rediss://") {
            if opt, err := redis.ParseURL(url); err == nil {
                redisClient = redis.NewClient(opt)
                return
            }
        }
        redisClient = redis.NewClient(&redis.Options{Addr: url})
    })
    return redisClient
}

func publishEvent(ctx context.Context, stream string, payload string) error {
    return getRedis().XAdd(ctx, &redis.XAddArgs{
        Stream: stream,
        Values: map[string]any{"payload": payload},
    }).Err()
}

type eventHandlers[T any] struct {
    sync.Mutex
    list []func(T)
}

func handlersSnapshot[T any](h *eventHandlers[T]) []func(T) {
    h.Lock()
    list := append([]func(T){}, h.list...)
    h.Unlock()
    return list
}

func consumeLoop[T any](stream string, handlers *eventHandlers[T]) {
    ctx := context.Background()
    rdb := getRedis()
    group := groupName()
    consumer := consumerName()
    if err := rdb.XGroupCreateMkStream(ctx, stream, group, "$").Err(); err != nil {
        if !strings.Contains(err.Error(), "BUSYGROUP") {
            fmt.Println("events: group create error", err)
        }
    }
    for {
        streams, err := rdb.XReadGroup(ctx, &redis.XReadGroupArgs{
            Group:    group,
            Consumer: consumer,
            Streams:  []string{stream, ">"},
            Count:    10,
            Block:    time.Second,
        }).Result()
        if err != nil {
            if err == redis.Nil {
                continue
            }
            time.Sleep(500 * time.Millisecond)
            continue
        }
        for _, s := range streams {
            for _, msg := range s.Messages {
                payloadRaw, ok := msg.Values["payload"]
                if !ok {
                    _ = rdb.XAck(ctx, stream, group, msg.ID).Err()
                    continue
                }
                payloadStr := ""
                switch v := payloadRaw.(type) {
                case string:
                    payloadStr = v
                case []byte:
                    payloadStr = string(v)
                default:
                    payloadStr = fmt.Sprint(v)
                }
                var parsed T
                if err := json.Unmarshal([]byte(payloadStr), &parsed); err == nil {
                    for _, handler := range handlersSnapshot(handlers) {
                        handler(parsed)
                    }
                }
                _ = rdb.XAck(ctx, stream, group, msg.ID).Err()
            }
        }
    }
}
"#,
    );

    for ev in &service.events.definitions {
        let name = ev.name.code_name();
        let payload_ty = go_type(&ev.payload, true);
        let handlers = format!("{}_Handlers", name);
        out.push_str(&format!(
            "var {handlers} = eventHandlers[{payload_ty}]{{}}\n",
            handlers = handlers,
            payload_ty = payload_ty
        ));
        out.push_str(&format!(
            "func On{name}(handler func({payload_ty})) {{\n    {handlers}.Lock()\n    defer {handlers}.Unlock()\n    {handlers}.list = append({handlers}.list, handler)\n}}\n",
            name = name,
            payload_ty = payload_ty,
            handlers = handlers
        ));
        out.push_str(&format!(
            "func Emit{name}(payload {payload_ty}) {{\n    data, err := json.Marshal(payload)\n    if err != nil {{\n        return\n    }}\n    stream := streamName(\"{name}\")\n    go func() {{\n        if err := publishEvent(context.Background(), stream, string(data)); err != nil {{\n            fmt.Println(\"events: publish error\", err)\n        }}\n    }}()\n}}\n\n",
            name = name,
            payload_ty = payload_ty
        ));
    }

    if !service.events.subscribes.is_empty() {
        out.push_str("func StartEventConsumers() {\n");
        for ev in &service.events.subscribes {
            let name = ev.code_name();
            let payload_ty = service
                .events
                .definitions
                .iter()
                .find(|e| e.name.code_name() == name)
                .map(|e| go_type(&e.payload, true))
                .unwrap_or_else(|| "map[string]interface{}".to_string());
            let handlers = format!("{}_Handlers", name);
            out.push_str(&format!(
                "    go consumeLoop[{payload_ty}](streamName(\"{name}\"), &{handlers})\n",
                payload_ty = payload_ty,
                name = name,
                handlers = handlers
            ));
        }
        out.push_str("}\n");
    }

    out
}
