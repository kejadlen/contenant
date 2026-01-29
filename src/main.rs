use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use tracing_subscriber::EnvFilter;

use contenant::Contenant;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run Claude Code in a container
    Run,
}

fn main() -> Result<std::process::ExitCode> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Run) {
        Command::Run => {
            let cwd = std::env::current_dir()?;
            let exit_code = Contenant::new(&cwd)?.run()?;
            Ok(std::process::ExitCode::from(exit_code as u8))
        }
    }
}
