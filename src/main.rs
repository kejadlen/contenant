use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
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
        #[arg(last = true, add = ArgValueCompleter::new(complete_claude_args))]
        claude_args: Vec<String>,
    },
    /// Start the host command bridge server
    Bridge,
}

fn complete_claude_args(current: &OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_str().unwrap_or_default();

    // Only complete flags
    if !current.starts_with('-') {
        return vec![];
    }

    let Ok(output) = ProcessCommand::new("claude").arg("--help").output() else {
        return vec![];
    };
    let Ok(help) = String::from_utf8(output.stdout) else {
        return vec![];
    };

    let mut candidates = vec![];
    for line in help.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('-') {
            continue;
        }

        // Parse flags from help lines like "  -p, --print  Description"
        for part in trimmed.split_whitespace() {
            let flag = part.trim_end_matches(',');
            if flag.starts_with('-') && flag.starts_with(current) {
                // Extract description: everything after the last flag+value
                let help_text = trimmed
                    .split_whitespace()
                    .skip_while(|w| w.starts_with('-') || w.starts_with('<'))
                    .collect::<Vec<_>>()
                    .join(" ");

                let mut candidate = CompletionCandidate::new(flag);
                if !help_text.is_empty() {
                    candidate = candidate.help(Some(help_text.into()));
                }
                candidates.push(candidate);
            }
        }
    }

    candidates
}

fn main() -> Result<std::process::ExitCode> {
    color_eyre::install()?;

    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

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
