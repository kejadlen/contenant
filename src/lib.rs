use std::fs;
use std::process::Command;

use color_eyre::eyre::{Result, bail};
use tracing::info;

const DOCKERFILE: &str = include_str!("../image/Dockerfile");
const IMAGE_HASH: &str = env!("IMAGE_HASH");
const IMAGE_NAME: &str = "contenant:latest";

pub trait Backend {
    fn is_current(&self) -> bool;
    fn build(&self) -> Result<()>;
    fn run(&self) -> Result<()>;
}

pub struct Docker;

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
        let dockerfile_path = xdg_dirs.place_cache_file("Dockerfile")?;
        fs::write(&dockerfile_path, DOCKERFILE)?;

        let cache_dir = dockerfile_path.parent().unwrap();

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
        let cwd = std::env::current_dir()?;
        let status = Command::new("docker")
            .args([
                "run",
                "-it",
                "--rm",
                "-v",
                &format!("{}:/workspace", cwd.display()),
                "-w",
                "/workspace",
                IMAGE_NAME,
            ])
            .status()?;

        let Some(code) = status.code() else {
            bail!("Container terminated by signal");
        };

        std::process::exit(code);
    }
}

pub struct Contenant<B = Docker> {
    backend: B,
}

impl Default for Contenant<Docker> {
    fn default() -> Self {
        Self { backend: Docker }
    }
}

impl<B: Backend> Contenant<B> {
    pub fn run(&self) -> Result<()> {
        if !self.backend.is_current() {
            self.backend.build()?;
        }
        self.backend.run()
    }
}
