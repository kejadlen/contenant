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
}
