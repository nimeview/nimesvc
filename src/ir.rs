#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub common: ProjectCommon,
    pub services: Vec<Service>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCommon {
    pub output: Option<String>,
    pub modules: Vec<ModuleUse>,
    pub auth: Option<AuthSpec>,
    pub middleware: Vec<String>,
    pub default_version: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    pub language: Option<Lang>,
    pub common: ServiceCommon,
    pub schema: ServiceSchema,
    pub http: ServiceHttp,
    pub rpc: ServiceRpc,
    pub sockets: ServiceSockets,
    pub events: ServiceEvents,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceCommon {
    pub modules: Vec<ModuleUse>,
    pub auth: Option<AuthSpec>,
    pub middleware: Vec<String>,
    pub address: Option<String>,
    pub port: Option<u16>,
    pub base_url: Option<String>,
    pub cors: Option<CorsConfig>,
    pub rate_limit: Option<RateLimit>,
    pub env: Vec<EnvVar>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSchema {
    pub types: Vec<TypeDef>,
    pub enums: Vec<EnumDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceHttp {
    pub headers: Vec<Field>,
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRpc {
    pub grpc_config: Option<GrpcConfig>,
    pub methods: Vec<RpcDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSockets {
    pub sockets: Vec<SocketDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEvents {
    pub config: Option<EventsConfig>,
    pub definitions: Vec<EventDef>,
    pub emits: Vec<TypeName>,
    pub subscribes: Vec<TypeName>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvVar {
    pub name: String,
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventsBroker {
    Redis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventsConfig {
    pub broker: EventsBroker,
    pub url: Option<String>,
    pub group: Option<String>,
    pub consumer: Option<String>,
    pub stream_prefix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorsConfig {
    pub allow_any: bool,
    pub origins: Vec<String>,
    pub methods: Vec<String>,
    pub headers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub method: HttpMethod,
    pub path: String,
    pub input: Input,
    pub responses: Vec<ResponseSpec>,
    pub auth: Option<AuthSpec>,
    pub middleware: Vec<String>,
    pub call: CallSpec,
    pub rate_limit: Option<RateLimit>,
    pub healthcheck: bool,
    pub headers: Vec<Field>,
    pub internal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
}

impl HttpMethod {
    pub fn from_str(raw: &str) -> Option<Self> {
        match raw {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            "OPTIONS" => Some(Self::Options),
            "HEAD" => Some(Self::Head),
            _ => None,
        }
    }

    pub fn as_openapi_key(&self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Put => "put",
            Self::Patch => "patch",
            Self::Delete => "delete",
            Self::Options => "options",
            Self::Head => "head",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    String,
    Int,
    Float,
    Bool,
    Object(Vec<Field>),
    Array(Box<Type>),
    Map(Box<Type>),
    Union(Vec<Type>),
    OneOf(Vec<Type>),
    Nullable(Box<Type>),
    Void,
    Any,
    Named(TypeName),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseSpec {
    pub status: u16,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Input {
    pub path: Vec<Field>,
    pub query: Vec<Field>,
    pub body: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub optional: bool,
    pub validation: Option<Validation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef {
    pub name: TypeName,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: TypeName,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: String,
    pub value: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDef {
    pub name: TypeName,
    pub payload: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcDef {
    pub service: String,
    pub name: String,
    pub version: Option<u32>,
    pub input: Vec<Field>,
    pub headers: Vec<Field>,
    pub output: Type,
    pub call: CallSpec,
    pub modules: Vec<ModuleUse>,
    pub auth: Option<AuthSpec>,
    pub middleware: Vec<String>,
    pub rate_limit: Option<RateLimit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketDef {
    pub name: TypeName,
    pub path: String,
    pub rooms: Vec<String>,
    pub triggers: Vec<SocketTrigger>,
    pub inbound: Vec<SocketMessage>,
    pub outbound: Vec<SocketMessage>,
    pub modules: Vec<ModuleUse>,
    pub auth: Option<AuthSpec>,
    pub middleware: Vec<String>,
    pub rate_limit: Option<RateLimit>,
    pub headers: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketMessage {
    pub name: TypeName,
    pub handler: CallSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketTrigger {
    pub name: TypeName,
    pub room: Option<String>,
    pub payload: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeName {
    pub name: String,
    pub version: Option<u32>,
}

impl TypeName {
    pub fn display(&self) -> String {
        match self.version {
            Some(v) => format!("{}@{}", self.name, v),
            None => self.name.clone(),
        }
    }

    pub fn code_name(&self) -> String {
        match self.version {
            Some(v) => format!("{}V{}", self.name, v),
            None => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validation {
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub min_len: Option<i64>,
    pub max_len: Option<i64>,
    pub min_items: Option<i64>,
    pub max_items: Option<i64>,
    pub regex: Option<String>,
    pub format: Option<String>,
    pub constraints: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSpec {
    None,
    Bearer,
    ApiKey,
}

pub fn effective_auth<'a>(
    route_auth: Option<&'a AuthSpec>,
    service_auth: Option<&'a AuthSpec>,
) -> Option<&'a AuthSpec> {
    match route_auth {
        Some(AuthSpec::None) => None,
        Some(auth) => Some(auth),
        None => match service_auth {
            Some(AuthSpec::None) => None,
            other => other,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSpec {
    pub service: Option<String>,
    pub service_base: Option<String>,
    pub module: String,
    pub function: String,
    pub args: Vec<CallArg>,
    pub is_async: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallArg {
    pub name: Option<String>,
    pub value: InputRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputRef {
    pub source: InputSource,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSource {
    Path,
    Query,
    Body,
    Headers,
    Input,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimit {
    pub max: u32,
    pub per_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcConfig {
    pub address: Option<String>,
    pub port: Option<u16>,
    pub max_message_size: Option<u64>,
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lang {
    Rust,
    TypeScript,
    Go,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleUse {
    pub name: String,
    pub path: Option<String>,
    pub version: Option<String>,
    pub alias: Option<String>,
    pub scope: UseScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseScope {
    Both,
    Compile,
    Runtime,
}
