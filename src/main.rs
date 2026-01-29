use std::path::PathBuf;

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
    Run {
        /// Project directory to mount (defaults to current directory)
        path: Option<PathBuf>,
    },
}

fn main() -> Result<std::process::ExitCode> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Run { path: None }) {
        Command::Run { path } => {
            let project_dir = match path {
                Some(p) => p,
                None => std::env::current_dir()?,
            };
            let exit_code = Contenant::new(&project_dir)?.run()?;
            Ok(std::process::ExitCode::from(exit_code as u8))
        }
    }
}
