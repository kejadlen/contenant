use facet::Facet;
use facet_kdl as kdl;
use std::fs;
use std::path::PathBuf;

#[derive(Facet, Debug, Clone)]
pub struct Mount {
    #[facet(kdl::arguments)]
    paths: Vec<String>,
    #[facet(kdl::property)]
    #[facet(default)]
    readonly: bool,
}

impl Mount {
    pub fn src(&self) -> &str {
        &self.paths[0]
    }

    pub fn dst(&self) -> &str {
        &self.paths[1]
    }

    pub fn readonly(&self) -> bool {
        self.readonly
    }
}

#[derive(Facet, Debug, Default)]
pub struct Config {
    #[facet(kdl::children)]
    #[facet(default)]
    mounts: Vec<Mount>,
}

impl Config {
    pub fn load() -> Self {
        let config_path = Self::config_path();
        if !config_path.exists() {
            return Config::default();
        }

        let content = match fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: Failed to read config file: {}", e);
                return Config::default();
            }
        };

        match facet_kdl::from_str(&content) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Warning: Failed to parse config file: {}", e);
                Config::default()
            }
        }
    }

    fn config_path() -> PathBuf {
        let xdg = xdg::BaseDirectories::with_prefix("contenant");
        xdg.get_config_home()
            .expect("HOME not set")
            .join("config.kdl")
    }

    pub fn mounts(&self) -> &[Mount] {
        &self.mounts
    }
}
