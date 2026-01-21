use std::process::Command;

const IMAGE: &str = "contenant:latest";

fn main() {
    let project_path = std::env::current_dir().expect("Failed to get current directory");

    let xdg = xdg::BaseDirectories::with_prefix("contenant");
    let claude_state_dir = xdg.create_data_directory("claude").expect("Failed to create claude state directory");

    let project_mount = format!("type=bind,src={},dst=/project", project_path.display());
    let claude_mount = format!("type=bind,src={},dst=/home/claude/.claude", claude_state_dir.display());

    let status = Command::new("container")
        .args([
            "run",
            "-it",
            "--rm",
            "--workdir", "/project",
            "--mount", &project_mount,
            "--mount", &claude_mount,
            IMAGE,
        ])
        .status()
        .expect("Failed to run container");

    std::process::exit(status.code().unwrap_or(1));
}
