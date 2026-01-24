use mlua::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FENNEL_SRC: &str = include_str!("fennel/fennel.lua");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Mount {
    src: String,
    dst: String,
    #[serde(default)]
    readonly: bool,
}

impl Mount {
    pub fn src(&self) -> &str {
        &self.src
    }

    pub fn dst(&self) -> &str {
        &self.dst
    }

    pub fn readonly(&self) -> bool {
        self.readonly
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    mounts: Vec<Mount>,
}

impl Config {
    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }

    fn try_load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::config_path();
        if !config_path.exists() {
            return Ok(Config::default());
        }

        let lua = Lua::new();

        // Load Fennel compiler (returns module table)
        let fennel: LuaTable = lua.load(FENNEL_SRC).eval()?;

        // Run user config and deserialize result
        let dofile: LuaFunction = fennel.get("dofile")?;
        let result: LuaValue = dofile.call(config_path.to_string_lossy().as_ref())?;

        Ok(lua.from_value(result)?)
    }

    fn config_path() -> PathBuf {
        let xdg = xdg::BaseDirectories::with_prefix("contenant");
        xdg.get_config_home()
            .expect("HOME not set")
            .join("config.fnl")
    }

    pub fn mounts(&self) -> &[Mount] {
        &self.mounts
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_fennel_eval() {
        let lua = Lua::new();

        // Fennel returns the module table; assign to global
        let fennel: LuaTable = lua.load(FENNEL_SRC).eval().expect("Failed to load fennel");
        let eval: LuaFunction = fennel.get("eval").unwrap();

        let result: LuaTable = eval.call("{:mounts []}").unwrap();
        assert!(result.contains_key("mounts").unwrap());
    }

    #[test]
    fn test_fennel_config_deserialize() {
        let lua = Lua::new();

        let fennel: LuaTable = lua.load(FENNEL_SRC).eval().expect("Failed to load fennel");
        let eval: LuaFunction = fennel.get("eval").unwrap();

        let code = r#"{:mounts [{:src "/src" :dst "/app"} {:src "~/.config" :dst "/home/user/.config" :readonly true}]}"#;
        let result: LuaTable = eval.call(code).unwrap();

        let config: Config = lua.from_value(LuaValue::Table(result)).unwrap();
        assert_eq!(config.mounts.len(), 2);
        assert_eq!(config.mounts[0].src(), "/src");
        assert_eq!(config.mounts[0].dst(), "/app");
        assert!(!config.mounts[0].readonly());
        assert_eq!(config.mounts[1].src(), "~/.config");
        assert!(config.mounts[1].readonly());
    }
}
