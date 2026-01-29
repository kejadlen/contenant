use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use color_eyre::eyre::{OptionExt, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::info;

const DOCKERFILE: &str = include_str!("../image/Dockerfile");
const CLAUDE_JSON: &str = include_str!("../image/claude.json");

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub mounts: Vec<Mount>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct Mount {
    pub source: String,
    pub target: Option<String>,
    #[serde(default)]
    pub readonly: bool,
}

const CONTAINER_HOME: &str = "/home/claude";

impl Mount {
    /// Format as a Docker volume mount string.
    ///
    /// Relative source paths are resolved from `config_dir`.
    pub fn to_docker_volume(&self, config_dir: &Path) -> String {
        let host_home = || dirs::home_dir().map(|p| p.to_string_lossy().into_owned());
        let container_home = || Some(CONTAINER_HOME.to_string());

        let source = shellexpand::tilde_with_context(&self.source, host_home);
        let target_str = self.target.as_deref().unwrap_or(&self.source);
        let target = shellexpand::tilde_with_context(target_str, container_home);

        let source_path = Path::new(source.as_ref());
        let source = if source_path.is_relative() {
            config_dir.join(source_path).to_string_lossy().into_owned()
        } else {
            source.into_owned()
        };

        let suffix = if self.readonly { ":ro" } else { "" };
        format!("{}:{}{}", source, target, suffix)
    }
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
    fn tag(&self, source: &str, target: &str) -> Result<()>;
    fn run(&self, image: &str, mounts: &[String], env: &HashMap<String, String>) -> Result<i32>;
}

pub struct Docker;

impl Backend for Docker {
    fn build(&self, tag: &str, path: &Path) -> Result<()> {
        info!(tag, "Building image");

        let path = path
            .to_str()
            .ok_or_eyre("build context path is not valid UTF-8")?;
        let status = Command::new("docker")
            .args(["build", "-t", tag, path])
            .status()?;

        if !status.success() {
            bail!("Docker build failed");
        }

        Ok(())
    }

    fn tag(&self, source: &str, target: &str) -> Result<()> {
        info!(source, target, "Tagging image");

        let status = Command::new("docker")
            .args(["tag", source, target])
            .status()?;

        if !status.success() {
            bail!("Docker tag failed");
        }

        Ok(())
    }

    fn run(&self, tag: &str, mounts: &[String], env: &HashMap<String, String>) -> Result<i32> {
        let cwd = std::env::current_dir()?;

        let mut cmd = Command::new("docker");
        cmd.args(["run", "-it", "--rm"]);
        cmd.args(["-v", &format!("{}:/workspace", cwd.display())]);

        for mount in mounts {
            cmd.args(["-v", mount]);
        }

        for (key, value) in env {
            cmd.args(["-e", &format!("{}={}", key, value)]);
        }

        cmd.args(["-w", "/workspace", tag]);

        let status = cmd.status()?;

        let Some(code) = status.code() else {
            bail!("Container terminated by signal");
        };

        Ok(code)
    }
}

pub struct Contenant<B = Docker> {
    backend: B,
    config: Config,
    app_dirs: xdg::BaseDirectories,
    project_dir: std::path::PathBuf,
}

impl<B> Contenant<B> {
    fn project_id(&self) -> String {
        let hash = format!(
            "{:x}",
            Sha256::digest(self.project_dir.as_os_str().as_encoded_bytes())
        );
        let short_hash = &hash[..8];
        let name = self.project_dir.file_name().unwrap().to_string_lossy();

        format!("{}-{}", short_hash, name)
    }
}

impl Contenant<Docker> {
    pub fn new(project_dir: &Path) -> Result<Self> {
        let app_dirs = xdg::BaseDirectories::with_prefix("contenant");
        let project_dir = std::fs::canonicalize(project_dir)?;
        Ok(Self {
            backend: Docker,
            config: Config::load(&app_dirs)?,
            app_dirs,
            project_dir,
        })
    }
}

impl<B: Backend> Contenant<B> {
    pub fn run(&self) -> Result<i32> {
        // Build base image (Docker cache handles unchanged builds)
        let dockerfile_path = self.app_dirs.place_cache_file("Dockerfile")?;
        fs::write(&dockerfile_path, DOCKERFILE)?;
        let claude_json_path = self.app_dirs.place_cache_file("claude.json")?;
        fs::write(&claude_json_path, CLAUDE_JSON)?;

        let context = self.app_dirs.get_cache_home().unwrap();
        self.backend.build("contenant:base", &context)?;

        // Build user image if a user Dockerfile exists, otherwise tag base as user
        let mut run_image = String::from("contenant:user");
        if let Some(user_dockerfile) = self.app_dirs.find_config_file("Dockerfile") {
            let context = user_dockerfile.parent().unwrap();
            self.backend.build("contenant:user", context)?;
        } else {
            self.backend.tag("contenant:base", "contenant:user")?;
        }

        // Build project image if .contenant/Dockerfile exists
        let project_dockerfile = self.project_dir.join(".contenant/Dockerfile");
        if project_dockerfile.exists() {
            let context = project_dockerfile.parent().unwrap();
            run_image = format!("contenant:{}", self.project_id());
            self.backend.build(&run_image, context)?;
        }

        // Default mount: persist Claude state (auth, settings, etc.)
        let claude_state_dir = self.app_dirs.place_state_file("claude")?;
        fs::create_dir_all(&claude_state_dir)?;
        let mut mounts = vec![format!(
            "{}:{}/.claude",
            claude_state_dir.display(),
            CONTAINER_HOME
        )];

        // User-defined mounts (can shadow subdirectories of defaults)
        let config_dir = self.app_dirs.get_config_home().unwrap();
        let user_mounts: Vec<_> = self
            .config
            .mounts
            .iter()
            .map(|mount| mount.to_docker_volume(&config_dir))
            .collect();
        mounts.extend(user_mounts);

        let env: HashMap<_, _> = self
            .config
            .env
            .iter()
            .map(|(key, value)| {
                let value =
                    shellexpand::tilde_with_context(value, || Some(CONTAINER_HOME.to_string()));
                (key.clone(), value.into_owned())
            })
            .collect();

        self.backend.run(&run_image, &mounts, &env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_absolute_paths() {
        let mount = Mount {
            source: "/host/path".to_string(),
            target: Some("/container/path".to_string()),
            readonly: false,
        };
        assert_eq!(
            mount.to_docker_volume(Path::new("/config")),
            "/host/path:/container/path"
        );
    }

    #[test]
    fn mount_target_defaults_to_source() {
        let mount = Mount {
            source: "/shared/path".to_string(),
            target: None,
            readonly: false,
        };
        assert_eq!(
            mount.to_docker_volume(Path::new("/config")),
            "/shared/path:/shared/path"
        );
    }

    #[test]
    fn mount_tilde_in_target_expands_to_container_home() {
        let mount = Mount {
            source: "/host/path".to_string(),
            target: Some("~/.config".to_string()),
            readonly: false,
        };
        assert_eq!(
            mount.to_docker_volume(Path::new("/config")),
            "/host/path:/home/claude/.config"
        );
    }

    #[test]
    fn mount_tilde_target_defaults_to_source_with_container_home() {
        let mount = Mount {
            source: "~/.ssh".to_string(),
            target: None,
            readonly: false,
        };
        let result = mount.to_docker_volume(Path::new("/config"));
        assert!(result.ends_with(":/home/claude/.ssh"));
    }

    #[test]
    fn mount_relative_source_resolved_from_config_dir() {
        let mount = Mount {
            source: "relative/path".to_string(),
            target: Some("/container/path".to_string()),
            readonly: false,
        };
        assert_eq!(
            mount.to_docker_volume(Path::new("/config")),
            "/config/relative/path:/container/path"
        );
    }

    #[test]
    fn mount_readonly() {
        let mount = Mount {
            source: "/host/path".to_string(),
            target: Some("/container/path".to_string()),
            readonly: true,
        };
        assert_eq!(
            mount.to_docker_volume(Path::new("/config")),
            "/host/path:/container/path:ro"
        );
    }
}
