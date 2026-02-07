use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;
use dirs::home_dir;
use serde::Deserialize;
use shellexpand::tilde_with_context;

pub const DEFAULT_BRIDGE_PORT: u16 = 19432;

pub const CONTAINER_HOME: &str = "/home/claude";

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub claude: ClaudeConfig,
    #[serde(default)]
    pub mounts: Vec<Mount>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub bridge: BridgeConfig,
}

#[derive(Debug, Deserialize)]
pub struct BridgeConfig {
    #[serde(default = "default_bridge_port")]
    pub port: u16,
    #[serde(default)]
    pub triggers: HashMap<String, String>,
}

fn default_bridge_port() -> u16 {
    DEFAULT_BRIDGE_PORT
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_BRIDGE_PORT,
            triggers: HashMap::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ClaudeConfig {
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Mount {
    pub source: String,
    pub target: Option<String>,
    #[serde(default = "default_readonly")]
    pub readonly: bool,
}

fn default_readonly() -> bool {
    true
}

impl Mount {
    /// Format as a Docker volume mount string.
    ///
    /// Relative source paths are resolved from `config_dir`.
    pub fn to_docker_volume(&self, config_dir: &Path) -> String {
        let host_home = || home_dir().map(|p| p.to_string_lossy().into_owned());
        let container_home = || Some(CONTAINER_HOME.to_string());

        let source = tilde_with_context(&self.source, host_home);
        let target_str = self.target.as_deref().unwrap_or(&self.source);
        let target = tilde_with_context(target_str, container_home);

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
    fn load_file(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config = serde_yaml_ng::from_str(&contents)?;
        Ok(config)
    }
}

/// Source of a configuration layer, ordered by precedence (lowest first).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConfigSource {
    /// Built-in defaults (lowest precedence).
    Default,
    /// User-level config (~/.config/contenant/config.yml).
    User,
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSource::Default => write!(f, "default"),
            ConfigSource::User => write!(f, "user"),
        }
    }
}

/// A single configuration layer with its source.
#[derive(Debug)]
pub struct ConfigLayer {
    pub source: ConfigSource,
    pub data: Config,
    /// Directory used to resolve relative mount source paths in this layer.
    pub config_dir: PathBuf,
}

/// Layered configuration that preserves all layers and resolves values on read.
///
/// Layers are stored in order of precedence (lowest first). Accessors walk
/// layers from highest to lowest precedence, taking the first value found
/// (for scalars/overrides) or accumulating across all layers (for additive
/// fields like mounts).
#[derive(Debug, Default)]
pub struct StackedConfig {
    layers: Vec<ConfigLayer>,
}

impl StackedConfig {
    /// Load all configuration layers.
    pub fn load(xdg_dirs: &xdg::BaseDirectories) -> Result<Self> {
        let mut config = Self::with_defaults();

        if let Some(config_path) = xdg_dirs.find_config_file("config.yml") {
            let config_dir = config_path.parent().unwrap().to_path_buf();
            let data = Config::load_file(&config_path)?;
            config.add_layer(ConfigSource::User, data, config_dir);
        }

        Ok(config)
    }

    /// Create a stack seeded with the built-in default layer.
    pub fn with_defaults() -> Self {
        let mut config = Self::default();
        // Default layer has no meaningful config dir; use root as placeholder.
        config.add_layer(ConfigSource::Default, Config::default(), PathBuf::from("/"));
        config
    }

    /// Add a layer at the position determined by its source precedence.
    pub fn add_layer(&mut self, source: ConfigSource, data: Config, config_dir: PathBuf) {
        let index = self.layers.partition_point(|layer| layer.source <= source);
        self.layers.insert(
            index,
            ConfigLayer {
                source,
                data,
                config_dir,
            },
        );
    }

    /// All layers, lowest precedence first.
    pub fn layers(&self) -> &[ConfigLayer] {
        &self.layers
    }

    /// Last layer to set `claude.version` wins.
    pub fn claude_version(&self) -> Option<&str> {
        self.layers
            .iter()
            .rev()
            .find_map(|l| l.data.claude.version.as_deref())
    }

    /// Mounts from all layers, lowest precedence first.
    ///
    /// Each mount is paired with the config directory of its layer, used to
    /// resolve relative source paths.
    pub fn mounts(&self) -> impl Iterator<Item = (&Mount, &Path)> {
        self.layers.iter().flat_map(|l| {
            l.data
                .mounts
                .iter()
                .map(move |m| (m, l.config_dir.as_path()))
        })
    }

    /// Env vars merged across layers; higher precedence overrides.
    pub fn env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        for layer in &self.layers {
            env.extend(layer.data.env.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        env
    }

    /// Bridge config merged across layers: last non-default port wins,
    /// triggers are merged with higher precedence overriding.
    pub fn bridge(&self) -> BridgeConfig {
        let port = self
            .layers
            .iter()
            .rev()
            .find(|l| l.data.bridge.port != DEFAULT_BRIDGE_PORT)
            .map_or(DEFAULT_BRIDGE_PORT, |l| l.data.bridge.port);

        let mut triggers = HashMap::new();
        for layer in &self.layers {
            triggers.extend(
                layer
                    .data
                    .bridge
                    .triggers
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        }

        BridgeConfig { port, triggers }
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

    #[test]
    fn bridge_config_defaults() {
        let config: BridgeConfig = serde_yaml_ng::from_str("{}").unwrap();
        assert_eq!(config.port, 19432);
        assert!(config.triggers.is_empty());
    }

    #[test]
    fn bridge_config_custom_port() {
        let config: BridgeConfig = serde_yaml_ng::from_str("port: 8080").unwrap();
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn bridge_config_with_triggers() {
        let yaml = r#"
triggers:
  open-editor: "code ."
  notify: "notify-send 'Done'"
"#;
        let config: BridgeConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.triggers.len(), 2);
        assert_eq!(
            config.triggers.get("open-editor"),
            Some(&"code .".to_string())
        );
        assert_eq!(
            config.triggers.get("notify"),
            Some(&"notify-send 'Done'".to_string())
        );
    }

    #[test]
    fn config_with_bridge_section() {
        let yaml = r#"
bridge:
  port: 9000
  triggers:
    test: "echo test"
"#;
        let config: Config = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.bridge.port, 9000);
        assert_eq!(
            config.bridge.triggers.get("test"),
            Some(&"echo test".to_string())
        );
    }

    #[test]
    fn stacked_config_defaults() {
        let config = StackedConfig::with_defaults();
        assert_eq!(config.claude_version(), None);
        assert_eq!(config.mounts().count(), 0);
        assert!(config.env().is_empty());
        assert_eq!(config.bridge().port, DEFAULT_BRIDGE_PORT);
        assert!(config.bridge().triggers.is_empty());
    }

    #[test]
    fn stacked_config_single_layer() {
        let mut config = StackedConfig::with_defaults();
        let layer: Config = serde_yaml_ng::from_str(
            r#"
claude:
  version: "1.0"
mounts:
  - source: /host/a
    target: /container/a
env:
  FOO: bar
bridge:
  port: 9000
  triggers:
    test: "echo test"
"#,
        )
        .unwrap();
        config.add_layer(ConfigSource::User, layer, PathBuf::from("/user-config"));

        assert_eq!(config.claude_version(), Some("1.0"));
        assert_eq!(config.mounts().count(), 1);
        assert_eq!(config.env().get("FOO").unwrap(), "bar");
        assert_eq!(config.bridge().port, 9000);
        assert_eq!(
            config.bridge().triggers.get("test"),
            Some(&"echo test".to_string())
        );
    }

    #[test]
    fn stacked_config_preserves_layers() {
        let mut config = StackedConfig::with_defaults();
        config.add_layer(
            ConfigSource::User,
            serde_yaml_ng::from_str(
                r#"
env:
  FOO: from-user
mounts:
  - source: /user/mount
"#,
            )
            .unwrap(),
            PathBuf::from("/user-config"),
        );

        assert_eq!(config.layers().len(), 2);
        assert_eq!(config.layers()[0].source, ConfigSource::Default);
        assert_eq!(config.layers()[1].source, ConfigSource::User);
        assert_eq!(
            config.layers()[1].data.env.get("FOO"),
            Some(&"from-user".to_string())
        );
    }

    #[test]
    fn stacked_config_mounts_carry_config_dir() {
        let mut config = StackedConfig::with_defaults();
        config.add_layer(
            ConfigSource::User,
            serde_yaml_ng::from_str(
                r#"
mounts:
  - source: relative/path
    target: /container/a
"#,
            )
            .unwrap(),
            PathBuf::from("/user-config"),
        );

        let mounts: Vec<_> = config.mounts().collect();
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].0.source, "relative/path");
        assert_eq!(mounts[0].1, Path::new("/user-config"));
    }
}
