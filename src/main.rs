use std::path::PathBuf;

use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use tracing_subscriber::EnvFilter;

use contenant::{Config, Contenant, bridge};

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

        /// Arguments to pass through to claude
        #[arg(last = true)]
        claude_args: Vec<String>,
    },
    /// Start the host command bridge server
    Bridge,
}

fn main() -> Result<std::process::ExitCode> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Run {
        path: None,
        claude_args: vec![],
    }) {
        Command::Run { path, claude_args } => {
            let project_dir = match path {
                Some(p) => p,
                None => std::env::current_dir()?,
            };
            let exit_code = Contenant::new(&project_dir)?.run(&claude_args)?;
            Ok(std::process::ExitCode::from(exit_code as u8))
        }
        Command::Bridge => {
            let xdg_dirs = xdg::BaseDirectories::with_prefix("contenant");
            let config = Config::load(&xdg_dirs)?;
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(bridge::serve(config.bridge.port, config.bridge.triggers))?;
            Ok(std::process::ExitCode::SUCCESS)
        }
    }
}
