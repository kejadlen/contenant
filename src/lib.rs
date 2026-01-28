use std::fs;
use std::path::Path;
use std::process::Command;

use color_eyre::eyre::{Result, bail};
use serde::Deserialize;
use tracing::info;

const DOCKERFILE: &str = include_str!("../image/Dockerfile");
const IMAGE_HASH: &str = env!("IMAGE_HASH");
const IMAGE_NAME: &str = "contenant:latest";

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub mounts: Vec<Mount>,
}

#[derive(Debug, Deserialize)]
pub struct Mount {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub readonly: bool,
}

impl Config {
    pub fn load(xdg_dirs: &xdg::BaseDirectories) -> Result<Self> {
        let Some(config_path) = xdg_dirs.find_config_file("config.yml") else {
            return Ok(Self::default());
        };

        let contents = fs::read_to_string(config_path)?;
        let config = serde_yaml_ng::from_str(&contents)?;
        Ok(config)
    }
}

pub trait Backend {
    fn is_current(&self) -> bool;
    fn build(&self, context: &Path) -> Result<()>;
    fn run(&self, config: &Config) -> Result<()>;
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

    fn build(&self, context: &Path) -> Result<()> {
        info!(hash = IMAGE_HASH, "Building image");

        let status = Command::new("docker")
            .args([
                "build",
                "--build-arg",
                &format!("IMAGE_HASH={}", IMAGE_HASH),
                "-t",
                IMAGE_NAME,
                context.to_str().unwrap(),
            ])
            .status()?;

        if !status.success() {
            bail!("Docker build failed");
        }

        Ok(())
    }

    fn run(&self, config: &Config) -> Result<()> {
        let cwd = std::env::current_dir()?;

        let mut cmd = Command::new("docker");
        cmd.args(["run", "-it", "--rm"]);
        cmd.args(["-v", &format!("{}:/workspace", cwd.display())]);

        for mount in &config.mounts {
            let suffix = if mount.readonly { ":ro" } else { "" };
            cmd.args([
                "-v",
                &format!("{}:{}{}", mount.source, mount.target, suffix),
            ]);
        }

        cmd.args(["-w", "/workspace", IMAGE_NAME]);

        let status = cmd.status()?;

        let Some(code) = status.code() else {
            bail!("Container terminated by signal");
        };

        std::process::exit(code);
    }
}

pub struct Contenant<B = Docker> {
    backend: B,
    config: Config,
    xdg_dirs: xdg::BaseDirectories,
}

impl Contenant<Docker> {
    pub fn new() -> Result<Self> {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("contenant");
        Ok(Self {
            backend: Docker,
            config: Config::load(&xdg_dirs)?,
            xdg_dirs,
        })
    }
}

impl<B: Backend> Contenant<B> {
    pub fn run(&self) -> Result<()> {
        if !self.backend.is_current() {
            // TODO Should this go into Contenant::init or something?
            let dockerfile_path = self.xdg_dirs.place_cache_file("Dockerfile")?;
            fs::write(&dockerfile_path, DOCKERFILE)?;
            let context = dockerfile_path.parent().unwrap();
            self.backend.build(context)?;
        }
        self.backend.run(&self.config)
    }
}
