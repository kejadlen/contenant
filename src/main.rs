mod backend;

use backend::{AppleContainer, Backend};
use serde::Deserialize;
use std::process::Command;

const IMAGE: &str = "contenant:latest";

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
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json = String::from_utf8(output.stdout).ok()?;
    let creds: Credentials = serde_json::from_str(&json).ok()?;
    Some(creds.claude_ai_oauth.access_token)
}

fn main() {
    let project_path = std::env::current_dir().expect("Failed to get current directory");
    let home_dir = std::env::var("HOME").expect("HOME not set");

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

    let args: Vec<String> = std::env::args().skip(1).collect();

    let backend = AppleContainer;
    let mut cmd = backend.command();
    cmd.args([
        "run",
        "-it",
        "--rm",
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

    let status = cmd.status().expect("Failed to run container");
    std::process::exit(status.code().unwrap_or(1));
}
