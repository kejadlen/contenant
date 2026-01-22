use std::process::Command;

pub trait Backend {
    fn command(&self) -> Command;
}

/// Backend using Apple's `container` CLI
pub struct AppleContainer;

impl Backend for AppleContainer {
    fn command(&self) -> Command {
        Command::new("container")
    }
}

/// Backend using Docker CLI
pub struct Docker;

impl Backend for Docker {
    fn command(&self) -> Command {
        Command::new("docker")
    }
}
