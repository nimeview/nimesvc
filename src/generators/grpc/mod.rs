use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::ir::{Lang, Service};

mod go;
mod rust;
mod ts;

pub fn generate_grpc_server(service: &Service, out_dir: &Path, lang: Lang) -> Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("Failed to create '{}'", out_dir.display()))?;

    match lang {
        Lang::Rust => rust::generate(service, out_dir),
        Lang::Go => go::generate(service, out_dir),
        Lang::TypeScript => ts::generate(service, out_dir),
    }
}
