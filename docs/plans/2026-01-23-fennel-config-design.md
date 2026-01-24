# Fennel Configuration Design

Replace facet-kdl with Fennel for configuration, using mlua for Lua/Fennel integration.

## Motivation

facet-kdl is deprecated with no ergonomic KDL alternative. Fennel provides a clean config syntax and aligns with flork.

## User-Facing API

**Config file:** `~/.config/contenant/config.fnl`

```fennel
{:mounts [{:src "/src" :dst "/app"}
          {:src "~/.config" :dst "/home/user/.config" :readonly true}]}
```

The config file returns a table that gets deserialized directly to Rust structs.

## Architecture

```
~/.config/contenant/config.fnl
         │
         ▼
    ┌─────────────────┐
    │  mlua + fennel  │  Embedded Lua 5.4 + Fennel compiler
    └────────┬────────┘
             │ fennel.dofile() returns Lua table
             ▼
    ┌─────────────────┐
    │  lua.from_value │  Deserialize to Rust via serde
    └────────┬────────┘
             │
             ▼
       Config struct
```

## Implementation

### Dependencies (Cargo.toml)

```toml
# Remove
facet = "0.42.0"
facet-kdl = "0.42.0"

# Add
mlua = { version = "*", features = ["lua54", "vendored", "serde"] }
serde = { version = "*", features = ["derive"] }
```

### Config Structs

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Mount {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub mounts: Vec<Mount>,
}
```

### Config Loading

```rust
const FENNEL_SRC: &str = include_str!("fennel/fennel.lua");

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

        // Load Fennel compiler
        lua.load(FENNEL_SRC).exec()?;

        // Run user config and deserialize result
        let fennel: LuaTable = lua.globals().get("fennel")?;
        let dofile: LuaFunction = fennel.get("dofile")?;
        let result: LuaValue = dofile.call(config_path.to_string_lossy().as_ref())?;

        Ok(lua.from_value(result)?)
    }

    fn config_path() -> PathBuf {
        xdg::BaseDirectories::with_prefix("contenant")
            .expect("HOME not set")
            .get_config_home()
            .join("config.fnl")
    }
}
```

## File Structure

```
src/
├── config.rs           # Rewritten with mlua
├── fennel/
│   └── fennel.lua      # Fennel compiler (~30KB)
└── main.rs
```

## Setup

Download Fennel compiler:
```bash
mkdir -p src/fennel
curl -o src/fennel/fennel.lua https://fennel-lang.org/downloads/fennel-1.6.1.lua
```

## Migration

- Old: `~/.config/contenant/config.kdl`
- New: `~/.config/contenant/config.fnl`

Old KDL files are ignored. Optionally warn if config.kdl exists but config.fnl doesn't.

## Implementation Steps

1. Update Cargo.toml dependencies
2. Download fennel.lua to src/fennel/
3. Rewrite src/config.rs with mlua implementation
4. Test with sample config.fnl
5. Remove facet-related code
