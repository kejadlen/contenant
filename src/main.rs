mod config;
mod runtime;

use clap::{Parser, Subcommand};
use runtime::Runtime;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;

const IMAGE: &str = "contenant:latest";
const IMAGE_HASH: &str = env!("IMAGE_HASH");

// Embedded image files
const DOCKERFILE: &str = include_str!("../image/Dockerfile");
const CLAUDE_JSON: &str = include_str!("../image/claude.json");
const JJ_SIGNING_TOML: &str = include_str!("../image/jj-container-signing.toml");

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
    /// Run a command in the container (default: claude)
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

/// Ensure the container image is built and up-to-date
fn ensure_image(runtime: &Runtime) {
    // Check if image exists with correct hash
    if let Some(current_hash) = runtime.get_image_hash(IMAGE) {
        if current_hash == IMAGE_HASH {
            return; // Image is up-to-date
        }
        eprintln!(
            "Image outdated (have {}, need {}), rebuilding...",
            current_hash, IMAGE_HASH
        );
    } else {
        eprintln!("Building container image...");
    }

    // Write embedded files to temp directory and build
    let temp_dir = std::env::temp_dir().join(format!("contenant-build-{}", IMAGE_HASH));
    fs::create_dir_all(&temp_dir).expect("Failed to create temp build directory");

    fs::write(temp_dir.join("Dockerfile"), DOCKERFILE).expect("Failed to write Dockerfile");
    fs::write(temp_dir.join("claude.json"), CLAUDE_JSON).expect("Failed to write claude.json");
    fs::write(temp_dir.join("jj-container-signing.toml"), JJ_SIGNING_TOML)
        .expect("Failed to write jj-container-signing.toml");

    if !runtime.build_image(IMAGE, &temp_dir, IMAGE_HASH) {
        eprintln!("Failed to build container image");
        std::process::exit(1);
    }

    // Clean up temp directory
    let _ = fs::remove_dir_all(&temp_dir);

    eprintln!("Image built successfully");
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

/// Create a new container (without starting it interactively)
fn create_container(
    runtime: &Runtime,
    container_id: &str,
    project_path: &Path,
    claude_state_dir: &Path,
    home_dir: &str,
    config: &config::Config,
) {
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

    let mut cmd = runtime.command();
    cmd.args([
        "create",
        "--name",
        container_id,
        "--workdir",
        "/project",
        "-it",
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
    ]);

    for mount in config.mounts() {
        let mount_spec = if mount.readonly() {
            format!("type=bind,src={},dst={},readonly", mount.src(), mount.dst())
        } else {
            format!("type=bind,src={},dst={}", mount.src(), mount.dst())
        };
        cmd.args(["--mount", &mount_spec]);
    }

    cmd.args([
        "--env",
        "SSH_AUTH_SOCK=/run/1password-agent.sock",
        "--entrypoint",
        "sleep",
        IMAGE,
        "infinity",
    ]);

    let status = cmd.status().expect("Failed to create container");

    if !status.success() {
        eprintln!("Failed to create container");
        std::process::exit(1);
    }
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

    // Ensure image is built and up-to-date
    ensure_image(&cli.runtime);

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

    // Extract command args, default to "claude"
    let args = match &cli.command {
        Some(Commands::Run { args }) if !args.is_empty() => args.clone(),
        _ => vec!["claude".to_string()],
    };

    // Load configuration
    let config = config::Config::load();

    // Ensure container exists
    if !cli.runtime.container_exists(&container_id) {
        create_container(
            &cli.runtime,
            &container_id,
            &project_path,
            &claude_state_dir,
            &home_dir,
            &config,
        );
    }

    // Start container, exec command, stop if no other sessions active
    cli.runtime.start_container(&container_id);
    let status = cli.runtime.exec_container(&container_id, &args);

    // Only stop if just the sleep process remains (no other exec sessions)
    if cli.runtime.container_process_count(&container_id) <= 1 {
        cli.runtime.stop_container(&container_id);
    }

    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_load() {
        let config = config::Config::load();
        println!("Loaded config: {:?}", config);
        for mount in config.mounts() {
            println!("Mount: {} -> {} (readonly: {})", mount.src(), mount.dst(), mount.readonly());
        }
    }
}
