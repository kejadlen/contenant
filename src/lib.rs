use std::fs;
use std::path::Path;
use std::process::Command;

use color_eyre::eyre::{Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::info;

const DOCKERFILE: &str = include_str!("../image/Dockerfile");
const CLAUDE_JSON: &str = include_str!("../image/claude.json");

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
    fn build(&self, image: &str, context: &Path) -> Result<()>;
    fn run(&self, image: &str, mounts: &[String]) -> Result<()>;
}

pub struct Docker;

impl Backend for Docker {
    fn build(&self, image: &str, context: &Path) -> Result<()> {
        info!(image, "Building image");

        let status = Command::new("docker")
            .args(["build", "-t", image, context.to_str().unwrap()])
            .status()?;

        if !status.success() {
            bail!("Docker build failed");
        }

        Ok(())
    }

    fn run(&self, image: &str, mounts: &[String]) -> Result<()> {
        let cwd = std::env::current_dir()?;

        let mut cmd = Command::new("docker");
        cmd.args(["run", "-it", "--rm"]);
        cmd.args(["-v", &format!("{}:/workspace", cwd.display())]);

        for mount in mounts {
            cmd.args(["-v", mount]);
        }

        cmd.args(["-w", "/workspace", image]);

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
    app_dirs: xdg::BaseDirectories,
    project_dirs: xdg::BaseDirectories,
}

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

fn project_id(dir: &Path) -> String {
    let hash = format!("{:x}", Sha256::digest(dir.as_os_str().as_encoded_bytes()));
    let short_hash = &hash[..8];
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{}-{}", short_hash, name)
}

impl Contenant<Docker> {
    pub fn new(project_dir: &Path) -> Result<Self> {
        let app_dirs = xdg::BaseDirectories::with_prefix("contenant");
        let project_dirs = xdg::BaseDirectories::with_profile("contenant", project_id(project_dir));
        Ok(Self {
            backend: Docker,
            config: Config::load(&app_dirs)?,
            app_dirs,
            project_dirs,
        })
    }
}

impl<B: Backend> Contenant<B> {
    pub fn run(&self) -> Result<()> {
        // Build base image (Docker cache handles unchanged builds)
        let dockerfile_path = self.app_dirs.place_cache_file("Dockerfile")?;
        fs::write(&dockerfile_path, DOCKERFILE)?;
        let claude_json_path = self.app_dirs.place_cache_file("claude.json")?;
        fs::write(&claude_json_path, CLAUDE_JSON)?;
        let context = dockerfile_path.parent().unwrap();
        self.backend.build("contenant:base", context)?;

        // Build user image if a user Dockerfile exists
        let mut run_image = "contenant:base";
        if let Some(user_dockerfile) = self.app_dirs.find_config_file("Dockerfile") {
            let context = user_dockerfile.parent().unwrap();
            self.backend.build("contenant:user", context)?;
            run_image = "contenant:user";
        }

        let config_dir = self
            .project_dirs
            .get_config_home()
            .map(|p| p.to_string_lossy().trim_end_matches('/').to_string());
        let container_home = "/home/claude".to_string();

        let context = |var: &str| -> Result<Option<String>, std::env::VarError> {
            Ok(match var {
                "CONTENANT_CONFIG_DIR" => config_dir.clone(),
                "CONTENANT_CONTAINER_HOME" => Some(container_home.clone()),
                _ => std::env::var(var).ok(),
            })
        };

        let mut mounts = self
            .config
            .mounts
            .iter()
            .map(|mount| {
                let home_dir = || dirs::home_dir().map(|p| p.to_string_lossy().into_owned());
                let source = shellexpand::full_with_context(&mount.source, home_dir, &context)?;
                let target = shellexpand::full_with_context(&mount.target, home_dir, &context)?;
                let suffix = if mount.readonly { ":ro" } else { "" };
                Ok(format!("{}:{}{}", source, target, suffix))
            })
            .collect::<Result<Vec<_>>>()?;

        // Sync credentials from macOS Keychain and mount as ~/.claude in the container
        let state_dir = self.app_dirs.create_state_directory("claude")?;
        // This will need to be configuration at some point. Also right now, we assume
        // the presence of an authed Claude on the host, which we shouldn't need eventually
        if let Some(creds) = get_credentials_json() {
            fs::write(state_dir.join(".credentials.json"), creds.trim())?;
        }
        mounts.push(format!("{}:/home/claude/.claude", state_dir.display()));

        self.backend.run(run_image, &mounts)
    }
}
