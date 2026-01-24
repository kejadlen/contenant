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

struct ContenantModule;

impl IntoLua for ContenantModule {
    fn into_lua(self, lua: &Lua) -> LuaResult<LuaValue> {
        let exports = lua.create_table()?;

        exports.set(
            "mount",
            lua.create_function(
                |lua, (src, dst, opts): (String, Option<String>, Option<LuaTable>)| {
                    let mount = Mount {
                        src: src.clone(),
                        dst: dst.unwrap_or_else(|| src.clone()),
                        readonly: opts
                            .and_then(|o| o.get("readonly").ok())
                            .unwrap_or(false),
                    };
                    lua.to_value(&mount)
                },
            )?,
        )?;

        exports.set("defaults", lua.to_value(&Config::default())?)?;

        Ok(LuaValue::Table(exports))
    }
}

impl Config {
    pub fn load() -> Self {
        Self::try_load().unwrap_or_else(|e| {
            eprintln!("Warning: Failed to load config: {}", e);
            Config::default()
        })
    }

    fn try_load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::config_path();
        eprintln!("DEBUG: config_path = {:?}", config_path);
        eprintln!("DEBUG: exists = {}", config_path.exists());
        if !config_path.exists() {
            return Ok(Config::default());
        }

        let lua = Lua::new();

        // Load Fennel compiler
        lua.load(FENNEL_SRC).exec()?;
        eprintln!("DEBUG: Fennel loaded");

        // Register contenant module
        let preload: LuaTable = lua.globals().get::<LuaTable>("package")?.get("preload")?;
        preload.set(
            "contenant",
            lua.create_function(|lua, ()| ContenantModule.into_lua(lua))?,
        )?;
        eprintln!("DEBUG: contenant module registered");

        // Run user config
        let fennel: LuaTable = lua.globals().get("fennel")?;
        let dofile: LuaFunction = fennel.get("dofile")?;
        eprintln!("DEBUG: calling dofile with {:?}", config_path.to_string_lossy().as_ref());
        let result: LuaValue = match dofile.call::<LuaValue>(config_path.to_string_lossy().as_ref()) {
            Ok(v) => {
                eprintln!("DEBUG: dofile returned {:?}", v.type_name());
                v
            }
            Err(e) => {
                eprintln!("DEBUG: dofile error: {}", e);
                return Err(e.into());
            }
        };

        eprintln!("DEBUG: result type = {:?}", result.type_name());
        if let LuaValue::Table(ref t) = result {
            eprintln!("DEBUG: table keys:");
            for pair in t.pairs::<String, LuaValue>() {
                if let Ok((k, v)) = pair {
                    eprintln!("  {} = {:?}", k, v.type_name());
                }
            }
        }

        Ok(lua.from_value(result)?)
    }

    fn config_path() -> PathBuf {
        xdg::BaseDirectories::with_prefix("contenant")
            .get_config_home()
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
        lua.load(FENNEL_SRC).exec().expect("Failed to load fennel");
        
        let fennel: LuaTable = lua.globals().get("fennel").unwrap();
        let eval: LuaFunction = fennel.get("eval").unwrap();
        let result: LuaValue = eval.call::<LuaValue>("{:mounts []}").unwrap();
        
        println!("Result type: {:?}", result.type_name());
        assert!(matches!(result, LuaValue::Table(_)));
    }

    #[test]
    fn test_contenant_module() {
        let lua = Lua::new();
        lua.load(FENNEL_SRC).exec().expect("Failed to load fennel");
        
        let preload: LuaTable = lua.globals().get::<LuaTable>("package").unwrap().get("preload").unwrap();
        preload.set(
            "contenant",
            lua.create_function(|lua, ()| ContenantModule.into_lua(lua)).unwrap(),
        ).unwrap();
        
        let fennel: LuaTable = lua.globals().get("fennel").unwrap();
        let eval: LuaFunction = fennel.get("eval").unwrap();
        
        // Test requiring contenant and using it
        let code = r#"
            (local c (require :contenant))
            (local config c.defaults)
            (table.insert config.mounts (c.mount "/src" "/dst"))
            config
        "#;
        
        let result: LuaValue = eval.call::<LuaValue>(code).unwrap();
        println!("Result type: {:?}", result.type_name());
        
        if let LuaValue::Table(t) = &result {
            let mounts: LuaTable = t.get("mounts").unwrap();
            let len = mounts.len().unwrap();
            println!("Mounts length: {}", len);
            assert_eq!(len, 1);
        } else {
            panic!("Expected table, got {:?}", result.type_name());
        }
    }
}
