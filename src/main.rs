mod cli;

fn main() -> anyhow::Result<()> {
    if let Err(err) = cli::run() {
        eprintln!("Error: {}", err);
        for cause in err.chain().skip(1) {
            eprintln!("Caused by: {}", cause);
        }
        std::process::exit(1);
    }
    Ok(())
}
