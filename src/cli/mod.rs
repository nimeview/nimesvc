use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod commands;
mod domains;

#[derive(Parser)]
#[command(name = "nimesvc")]
#[command(about = "Minimal DSL -> OpenAPI compiler", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Build {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    Lint {
        input: PathBuf,
    },
    Fmt {
        input: PathBuf,
        #[arg(long)]
        check: bool,
    },
    Generate {
        #[arg(value_name = "INPUT", index = 1, help = "Path to .ns file")]
        input: PathBuf,
        #[arg(
            value_name = "KIND",
            index = 2,
            help = "Server language (rust|ts|go) or kind (grpc). When omitted, gRPC is generated automatically for services with rpc."
        )]
        kind: Option<String>,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long, help = "Language when KIND is omitted or when KIND is grpc")]
        lang: Option<String>,
    },
    Run {
        #[arg(value_name = "INPUT", index = 1)]
        input: PathBuf,
        #[arg(
            value_name = "KIND",
            index = 2,
            help = "Server language (rust|ts|go) or kind (grpc). When omitted, gRPC is started automatically for services with rpc."
        )]
        kind: Option<String>,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long = "no-log")]
        no_log: bool,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    Dev {
        #[arg(value_name = "INPUT", index = 1)]
        input: PathBuf,
        #[arg(
            value_name = "KIND",
            index = 2,
            help = "Server language (rust|ts|go) or kind (grpc). When omitted, gRPC is started automatically for services with rpc."
        )]
        kind: Option<String>,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long = "no-log")]
        no_log: bool,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long, default_value_t = 500)]
        debounce_ms: u64,
    },
    Stop {
        #[arg(value_name = "INPUT", index = 1)]
        input: PathBuf,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    Env {
        #[arg(value_name = "INPUT", index = 1)]
        input: PathBuf,
    },
    Init,
    Doctor,
    Update {
        #[arg(long)]
        repo: Option<String>,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    commands::dispatch(cli)
}
