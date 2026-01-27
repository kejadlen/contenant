use std::fs;
use std::process::Command;

const DOCKERFILE: &str = include_str!("../image/Dockerfile");
const IMAGE_HASH: &str = env!("IMAGE_HASH");
const IMAGE_NAME: &str = "contenant:latest";

fn main() {
    if !image_is_current() {
        build_image();
    }
    run_container();
}

fn image_is_current() -> bool {
    let output = Command::new("docker")
        .args([
            "inspect",
            "--format",
            "{{index .Config.Labels \"contenant.hash\"}}",
            IMAGE_NAME,
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let label = String::from_utf8_lossy(&out.stdout).trim().to_string();
            label == IMAGE_HASH
        }
        _ => false,
    }
}

fn build_image() {
    eprintln!("Building image (hash: {})...", IMAGE_HASH);

    let xdg_dirs = xdg::BaseDirectories::with_prefix("contenant");
    let cache_dir = xdg_dirs
        .create_cache_directory("")
        .expect("Failed to create cache dir");

    let dockerfile_path = cache_dir.join("Dockerfile");
    fs::write(&dockerfile_path, DOCKERFILE).expect("Failed to write Dockerfile");

    let status = Command::new("docker")
        .args([
            "build",
            "--build-arg",
            &format!("IMAGE_HASH={}", IMAGE_HASH),
            "-t",
            IMAGE_NAME,
            cache_dir.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to run docker build");

    if !status.success() {
        eprintln!("Docker build failed");
        std::process::exit(1);
    }
}

fn run_container() {
    let status = Command::new("docker")
        .args(["run", "-it", "--rm", IMAGE_NAME])
        .status()
        .expect("Failed to run container");

    std::process::exit(status.code().unwrap_or(1));
}
