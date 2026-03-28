use anyhow::Result;

use super::{Cli, Commands};

mod build;
mod common;
mod dev;
mod doctor;
mod env;
mod fmt;
mod generate;
mod init;
mod lint;
mod run;
mod stop;
mod update;

pub(super) fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Build {
            input,
            output,
            json,
        } => build::build_cmd(input, output, json),
        Commands::Lint { input } => lint::lint_cmd(input),
        Commands::Fmt { input, check } => fmt::fmt_cmd(input, check),
        Commands::Generate {
            kind,
            input,
            out,
            lang,
        } => generate::generate_cmd(kind, input, out, lang),
        Commands::Run {
            kind,
            lang,
            no_log,
            input,
            out,
        } => run::run_cmd(kind, lang, no_log, input, out),
        Commands::Dev {
            kind,
            lang,
            no_log,
            input,
            out,
            debounce_ms,
        } => dev::dev_cmd(kind, lang, no_log, input, out, debounce_ms),
        Commands::Stop { input, out } => stop::stop_cmd(input, out),
        Commands::Init => init::init_cmd(),
        Commands::Doctor => doctor::doctor_cmd(),
        Commands::Env { input } => env::env_cmd(input),
        Commands::Update { repo } => update::update_cmd(repo),
    }
}
