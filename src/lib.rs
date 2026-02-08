pub mod bridge;
pub mod config;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::path::Path;
use std::process::Command;

use color_eyre::eyre::{OptionExt, Result, bail};
use hickory_resolver::TokioResolver;
use sha2::{Digest, Sha256};
use shellexpand::tilde_with_context;
use tempfile::NamedTempFile;
use tracing::info;

pub use config::StackedConfig;

use config::CONTAINER_HOME;

const DOCKERFILE: &str = include_str!("../assets/Dockerfile");
const CLAUDE_JSON: &str = include_str!("../assets/claude.json");

const ENTRYPOINT: &str = include_str!("../image/entrypoint.sh");

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
        // NET_ADMIN and NET_RAW are required for the entrypoint to configure nftables
        cmd.args([
            "run",
            "-it",
            "--rm",
            "--cap-add=NET_ADMIN",
            "--cap-add=NET_RAW",
        ]);
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

/// Resolve allowed domains to IPs/CIDRs and write them to a temp file.
///
/// The returned `NamedTempFile` must outlive the container process — dropping
/// it deletes the file. The caller should hold onto it until `backend.run()`
/// returns.
fn resolve_allowed_ips(domains: &[String]) -> Result<NamedTempFile> {
    let rt = tokio::runtime::Runtime::new()?;
    let resolver = TokioResolver::builder_tokio()?.build();
    let mut file = NamedTempFile::new()?;

    // If api.github.com is in the list, fetch GitHub's published CIDR ranges
    if domains.iter().any(|d| d == "api.github.com") {
        info!("Fetching GitHub IP ranges");
        let body: serde_json::Value = ureq::get("https://api.github.com/meta")
            .call()?
            .body_mut()
            .read_json()?;

        for key in &["web", "api", "git"] {
            if let Some(ranges) = body[key].as_array() {
                for range in ranges {
                    if let Some(cidr) = range.as_str() {
                        // Only include IPv4 CIDRs
                        if cidr.contains('.') {
                            info!(cidr, "Adding GitHub range");
                            writeln!(file, "{}", cidr)?;
                        }
                    }
                }
            }
        }
    }

    // Resolve each domain to A records
    for domain in domains {
        info!(domain, "Resolving domain");
        match rt.block_on(resolver.lookup_ip(domain.as_str())) {
            Ok(response) => {
                for ip in response.iter() {
                    if let IpAddr::V4(v4) = ip {
                        let entry = format!("{}/32", v4);
                        info!(entry, domain, "Adding IP");
                        writeln!(file, "{}", entry)?;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(domain, error = %e, "Failed to resolve domain");
            }
        }
    }

    file.flush()?;
    Ok(file)
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
        let entrypoint_path = self.app_dirs.place_cache_file("entrypoint.sh")?;
        fs::write(&entrypoint_path, ENTRYPOINT)?;

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

        // Resolve allowed domains and mount the IP file into the container
        let domains = self.config.allowed_domains();
        let allowed_ips_file = resolve_allowed_ips(domains)?;
        mounts.push(format!(
            "{}:/etc/contenant/allowed-ips:ro",
            allowed_ips_file.path().display()
        ));

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
