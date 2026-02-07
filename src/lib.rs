pub mod bridge;
pub mod config;

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use color_eyre::eyre::{OptionExt, Result, bail};
use sha2::{Digest, Sha256};
use shellexpand::tilde_with_context;
use tracing::info;

pub use config::StackedConfig;

use config::CONTAINER_HOME;

const DOCKERFILE: &str = include_str!("../assets/Dockerfile");
const CLAUDE_JSON: &str = include_str!("../assets/claude.json");

pub trait Backend {
    fn build(&self, image: &str, context: &Path) -> Result<()>;
    fn tag(&self, source: &str, target: &str) -> Result<()>;
    fn run(
        &self,
        image: &str,
        mounts: &[String],
        env: &HashMap<String, String>,
        args: &[String],
    ) -> Result<i32>;
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

    fn run(
        &self,
        tag: &str,
        mounts: &[String],
        env: &HashMap<String, String>,
        args: &[String],
    ) -> Result<i32> {
        let cwd = std::env::current_dir()?;

        let mut cmd = Command::new("docker");
        cmd.args(["run", "-it", "--rm"]);
        cmd.args(["--add-host", "host.docker.internal:host-gateway"]);
        cmd.args(["-v", &format!("{}:/workspace", cwd.display())]);

        for mount in mounts {
            cmd.args(["-v", mount]);
        }

        for (key, value) in env {
            cmd.args(["-e", &format!("{}={}", key, value)]);
        }

        cmd.args(["-w", "/workspace", tag]);
        cmd.args(args);

        let status = cmd.status()?;

        let Some(code) = status.code() else {
            bail!("Container terminated by signal");
        };

        Ok(code)
    }
}

pub struct Contenant<B = Docker> {
    backend: B,
    config: StackedConfig,
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
            config: StackedConfig::load(&app_dirs, Some(&project_dir))?,
            app_dirs,
            project_dir,
        })
    }
}

impl<B: Backend> Contenant<B> {
    pub fn run(&self, args: &[String]) -> Result<i32> {
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

        // Mount skills directory if it exists
        let config_dir = self.app_dirs.get_config_home().unwrap();
        let skills_dir = config_dir.join("skills");
        if skills_dir.exists() {
            mounts.push(format!(
                "{}:{}/.claude/skills",
                skills_dir.display(),
                CONTAINER_HOME
            ));
        }

        // Persist SSH known_hosts across sessions
        let known_hosts_file = self.app_dirs.place_state_file("ssh/known_hosts")?;
        if !known_hosts_file.exists() {
            fs::write(&known_hosts_file, "")?;
        }
        mounts.push(format!(
            "{}:{}/.ssh/known_hosts",
            known_hosts_file.display(),
            CONTAINER_HOME
        ));

        // User-defined mounts (can shadow subdirectories of defaults)
        let user_mounts: Vec<_> = self
            .config
            .mounts()
            .map(|(mount, config_dir)| mount.to_docker_volume(config_dir))
            .collect();
        mounts.extend(user_mounts);

        let mut env: HashMap<_, _> = self
            .config
            .env()
            .into_iter()
            .map(|(key, value)| {
                let value = tilde_with_context(&value, || Some(CONTAINER_HOME.to_string()));
                (key, value.into_owned())
            })
            .collect();

        let bridge = self.config.bridge();
        env.insert(
            "CONTENANT_BRIDGE_URL".to_string(),
            format!("http://host.docker.internal:{}", bridge.port),
        );

        self.backend.run(&run_image, &mounts, &env, args)
    }
}
