mod runtime;

use clap::{Parser, Subcommand};
use runtime::Runtime;
use sha2::{Digest, Sha256};
use std::fs;
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
    /// List all contenant containers
    List,
    /// Remove container(s)
    Clean {
        /// Path to project (defaults to current directory)
        path: Option<String>,
        /// Remove all contenant containers
        #[arg(long)]
        all: bool,
    },
    /// Run container (can also omit subcommand)
    Run {
        /// Command and arguments to run in the container
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

/// Get full credentials JSON from macOS Keychain
fn get_credentials_json() -> Option<String> {
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

    String::from_utf8(output.stdout).ok()
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

    // Handle list command
    if let Some(Commands::List) = cli.command {
        let containers = cli.runtime.list_containers("contenant-");
        if containers.is_empty() {
            println!("No contenant containers found");
        } else {
            println!("Contenant containers:");
            for container in containers {
                println!("  {}", container);
            }
        }
        return;
    }

    // Handle clean command
    if let Some(Commands::Clean { path, all }) = &cli.command {
        if *all {
            let containers = cli.runtime.list_containers("contenant-");
            if containers.is_empty() {
                println!("No contenant containers found");
            } else {
                for container in containers {
                    cli.runtime.remove_container(&container);
                    println!("Removed container: {}", container);
                }
            }
        } else {
            let target_path = if let Some(p) = path {
                Path::new(p)
                    .canonicalize()
                    .unwrap_or_else(|_| Path::new(p).to_path_buf())
            } else {
                project_path.clone()
            };
            let container_id = generate_container_id(&target_path);
            if cli.runtime.container_exists(&container_id) {
                cli.runtime.remove_container(&container_id);
                println!("Removed container: {}", container_id);
            } else {
                println!("No container found for this project");
            }
        }
        return;
    }

    let container_id = generate_container_id(&project_path);

    // Extract args from Run command, or use empty vec for default
    let args = match cli.command {
        Some(Commands::Run { args }) => args,
        _ => vec![],
    };

    // Set up claude state directory and sync credentials from host
    let xdg = xdg::BaseDirectories::with_prefix("contenant");
    let claude_state_dir = xdg
        .create_data_directory("claude")
        .expect("Failed to create claude state directory");

    // Sync credentials from macOS Keychain to container's credential file
    if let Some(creds) = get_credentials_json() {
        let creds_path = claude_state_dir.join(".credentials.json");
        fs::write(&creds_path, creds.trim()).expect("Failed to write credentials");
    }

    // Check if container already exists
    let status = if cli.runtime.container_exists(&container_id) {
        cli.runtime.start_container(&container_id)
    } else {
        // Create new container
        let project_mount = format!("type=bind,src={},dst=/project", project_path.display());
        let claude_mount = format!(
            "type=bind,src={},dst=/home/claude/.claude",
            claude_state_dir.display()
        );
        let skills_mount = format!(
            "type=bind,src={}/.claude/skills,dst=/home/claude/.claude/skills",
            home_dir
        );
        let jj_config_mount = format!(
            "type=bind,src={}/.config/jj/config.toml,dst=/home/claude/.config/jj/config.toml,readonly",
            home_dir
        );
        let ssh_agent_mount = format!(
            "type=bind,src={}/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock,dst=/run/1password-agent.sock",
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
            &jj_config_mount,
            "--mount",
            &ssh_agent_mount,
            "--env",
            "SSH_AUTH_SOCK=/run/1password-agent.sock",
        ]);

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
