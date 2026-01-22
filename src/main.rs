mod runtime;

use clap::{Parser, Subcommand};
use runtime::Runtime;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

const IMAGE: &str = "contenant:latest";

#[derive(Parser)]
#[command(name = "contenant")]
#[command(about = "Run Claude Code in a container")]
struct Cli {
    /// Container runtime to use
    #[arg(long, short, value_enum, default_value_t, global = true)]
    runtime: Runtime,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Remove the container for the current project
    Clean,
    /// Run container (can also omit subcommand)
    Run {
        /// Command and arguments to run in the container
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: OAuthCredentials,
}

#[derive(Deserialize)]
struct OAuthCredentials {
    #[serde(rename = "accessToken")]
    access_token: String,
}

fn get_oauth_token() -> Option<String> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json = String::from_utf8(output.stdout).ok()?;
    let creds: Credentials = serde_json::from_str(&json).ok()?;
    Some(creds.claude_ai_oauth.access_token)
}

fn generate_container_id(project_path: &Path) -> String {
    let basename = project_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project");

    let canonical = project_path
        .canonicalize()
        .unwrap_or_else(|_| project_path.to_path_buf());
    let path_str = canonical.display().to_string();

    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    let hash = hasher.finalize();
    let short_hash = format!("{:x}", hash)[..8].to_string();

    format!("contenant-{}-{}", basename, short_hash)
}

fn main() {
    let cli = Cli::parse();

    let project_path = std::env::current_dir().expect("Failed to get current directory");
    let home_dir = std::env::var("HOME").expect("HOME not set");

    let container_id = generate_container_id(&project_path);

    // Handle clean command
    if let Some(Commands::Clean) = cli.command {
        if cli.runtime.container_exists(&container_id) {
            cli.runtime.remove_container(&container_id);
            println!("Removed container: {}", container_id);
        } else {
            println!("No container found for this project");
        }
        return;
    }

    // Extract args from Run command, or use empty vec for default
    let args = match cli.command {
        Some(Commands::Run { args }) => args,
        _ => vec![],
    };

    // Check if container already exists
    let status = if cli.runtime.container_exists(&container_id) {
        cli.runtime.start_container(&container_id)
    } else {
        // Create new container
        let xdg = xdg::BaseDirectories::with_prefix("contenant");
        let claude_state_dir = xdg
            .create_data_directory("claude")
            .expect("Failed to create claude state directory");

        let project_mount = format!("type=bind,src={},dst=/project", project_path.display());
        let claude_mount = format!(
            "type=bind,src={},dst=/home/claude/.claude",
            claude_state_dir.display()
        );
        let skills_mount = format!(
            "type=bind,src={}/.claude/skills,dst=/home/claude/.claude/skills",
            home_dir
        );
        let commands_mount = format!(
            "type=bind,src={}/.claude/commands,dst=/home/claude/.claude/commands",
            home_dir
        );

        let mut cmd = cli.runtime.command();
        cmd.args([
            "run",
            "-it",
            "--name",
            &container_id,
            "--workdir",
            "/project",
            "--mount",
            &project_mount,
            "--mount",
            &claude_mount,
            "--mount",
            &skills_mount,
            "--mount",
            &commands_mount,
        ]);

        if let Some(token) = get_oauth_token() {
            cmd.args(["--env", &format!("CLAUDE_CODE_OAUTH_TOKEN={}", token)]);
        }

        if let Some((entrypoint, rest)) = args.split_first() {
            cmd.args(["--entrypoint", entrypoint, IMAGE]);
            cmd.args(rest);
        } else {
            cmd.arg(IMAGE);
        }

        cmd.status().expect("Failed to run container")
    };

    std::process::exit(status.code().unwrap_or(1));
}
