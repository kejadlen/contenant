# Fennel Configuration Design

Replace facet-kdl with Fennel for configuration, using mlua for Lua/Fennel integration.

## Motivation

facet-kdl is deprecated with no ergonomic KDL alternative. Fennel provides a clean config syntax and aligns with flork.

## User-Facing API

**Config file:** `~/.config/contenant/config.fnl`

```fennel
(local c (require :contenant))
(local config c.defaults)

(table.insert config.mounts (c.mount "/src" "/app"))
(table.insert config.mounts (c.mount "~/.config" "/home/user/.config" {:readonly true}))

config
```

**mount function signatures:**
- `(c.mount src)` - dst defaults to src
- `(c.mount src dst)` - explicit paths
- `(c.mount src dst {:readonly true})` - with options

**Starting fresh (no defaults):**
```fennel
(local c (require :contenant))

{:mounts [(c.mount "/only" "/this")]}
```

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
    Config { mounts: Vec<Mount> }
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

### Contenant Module (Rust → Lua)

```rust
struct ContenantModule;

impl IntoLua for ContenantModule {
    fn into_lua(self, lua: &Lua) -> LuaResult<LuaValue> {
        let exports = lua.create_table()?;

        exports.set("mount", lua.create_function(|lua, (src, dst, opts): (String, Option<String>, Option<LuaTable>)| {
            let mount = Mount {
                src: src.clone(),
                dst: dst.unwrap_or(src),
                readonly: opts.and_then(|o| o.get("readonly").ok()).unwrap_or(false),
            };
            lua.to_value(&mount)
        })?)?;

        exports.set("defaults", lua.to_value(&Config::default())?)?;

        Ok(LuaValue::Table(exports))
    }
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

        // Register contenant module
        let preload: LuaTable = lua.globals().get::<LuaTable>("package")?.get("preload")?;
        preload.set("contenant", lua.create_function(|lua, ()| {
            ContenantModule.into_lua(lua)
        })?)?;

        // Run user config
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
