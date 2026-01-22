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
}
