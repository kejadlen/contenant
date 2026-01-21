use std::process::Command;

const IMAGE: &str = "contenant:latest";

fn main() {
    let project_path = std::env::current_dir().expect("Failed to get current directory");

    let xdg = xdg::BaseDirectories::with_prefix("contenant");
    let claude_state_dir = xdg
        .create_data_directory("claude")
        .expect("Failed to create claude state directory");

    let project_mount = format!("type=bind,src={},dst=/project", project_path.display());
    let claude_mount = format!(
        "type=bind,src={},dst=/home/claude/.claude",
        claude_state_dir.display()
    );

    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut cmd = Command::new("container");
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
    ]);

    if let Some((entrypoint, rest)) = args.split_first() {
        cmd.args(["--entrypoint", entrypoint, IMAGE]);
        cmd.args(rest);
    } else {
        cmd.arg(IMAGE);
    }

    let status = cmd.status().expect("Failed to run container");
    std::process::exit(status.code().unwrap_or(1));
}
