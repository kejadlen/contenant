use std::path::Path;
use std::process::Command;

use clap::ValueEnum;

#[derive(Clone, Copy, Default, ValueEnum)]
pub enum Runtime {
    Apple,
    #[default]
    Docker,
}

impl Runtime {
    pub fn command(&self) -> Command {
        match self {
            Runtime::Apple => Command::new("container"),
            Runtime::Docker => Command::new("docker"),
        }
    }

    /// Get the hash label from an image, if it exists
    pub fn get_image_hash(&self, image: &str) -> Option<String> {
        let output = self
            .command()
            .args([
                "inspect",
                "--format",
                "{{index .Config.Labels \"contenant.hash\"}}",
                image,
            ])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if hash.is_empty() { None } else { Some(hash) }
    }

    /// Build an image from a directory
    pub fn build_image(&self, image: &str, build_dir: &Path, hash: &str) -> bool {
        let status = self
            .command()
            .args([
                "build",
                "-t",
                image,
                "--build-arg",
                &format!("IMAGE_HASH={}", hash),
                build_dir.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run build command");

        status.success()
    }

    pub fn container_exists(&self, name: &str) -> bool {
        let output = self.command().args(["inspect", name]).output().ok();

        output.map_or(false, |o| o.status.success())
    }

    pub fn start_container(&self, name: &str) -> std::process::ExitStatus {
        self.command()
            .args(["start", "-ai", name])
            .status()
            .expect("Failed to start container")
    }

    pub fn remove_container(&self, name: &str) {
        self.command()
            .args(["rm", "-f", name])
            .status()
            .expect("Failed to remove container");
    }

    pub fn list_containers(&self, prefix: &str) -> Vec<String> {
        let output = self
            .command()
            .args(["ps", "-a", "--format", "{{.Names}}"])
            .output()
            .expect("Failed to list containers");

        if !output.status.success() {
            return vec![];
        }

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.starts_with(prefix))
            .map(|s| s.to_string())
            .collect()
    }
}
