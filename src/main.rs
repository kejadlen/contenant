use std::fs;
use std::process::Command;

use color_eyre::eyre::{bail, Result};
use tracing::info;
use tracing_subscriber::EnvFilter;

const DOCKERFILE: &str = include_str!("../image/Dockerfile");
const IMAGE_HASH: &str = env!("IMAGE_HASH");
const IMAGE_NAME: &str = "contenant:latest";

trait Backend {
    fn is_current(&self) -> bool;
    fn build(&self) -> Result<()>;
    fn run(&self) -> Result<()>;
}

struct Docker;

impl Backend for Docker {
    fn is_current(&self) -> bool {
        let Ok(output) = Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{index .Config.Labels \"contenant.hash\"}}",
                IMAGE_NAME,
            ])
            .output()
        else {
            return false;
        };

        if !output.status.success() {
            return false;
        }

        let label = String::from_utf8_lossy(&output.stdout).trim().to_string();
        label == IMAGE_HASH
    }

    fn build(&self) -> Result<()> {
        info!(hash = IMAGE_HASH, "Building image");

        let xdg_dirs = xdg::BaseDirectories::with_prefix("contenant");
        let cache_dir = xdg_dirs.create_cache_directory("")?;

        let dockerfile_path = cache_dir.join("Dockerfile");
        fs::write(&dockerfile_path, DOCKERFILE)?;

        let status = Command::new("docker")
            .args([
                "build",
                "--build-arg",
                &format!("IMAGE_HASH={}", IMAGE_HASH),
                "-t",
                IMAGE_NAME,
                cache_dir.to_str().unwrap(),
            ])
            .status()?;

        if !status.success() {
            bail!("Docker build failed");
        }

        Ok(())
    }

    fn run(&self) -> Result<()> {
        let status = Command::new("docker")
            .args(["run", "-it", "--rm", IMAGE_NAME])
            .status()?;

        let Some(code) = status.code() else {
            bail!("Container terminated by signal");
        };

        std::process::exit(code);
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let backend = Docker;

    if !backend.is_current() {
        backend.build()?;
    }
    backend.run()?;

    Ok(())
}
