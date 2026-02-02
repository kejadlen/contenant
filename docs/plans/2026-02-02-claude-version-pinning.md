# Claude Version Pinning Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Allow users to pin the Claude Code version installed in the container via `config.yml`.

**Architecture:** Add a `claude` section to Config with a `version` field. Pass it as a Docker build arg to the embedded Dockerfile, which conditionally appends the version to the installer command. The `Backend::build` trait method gains a `build_args` parameter.

**Tech Stack:** Rust, serde, Docker ARG/build-arg

**Config example:**
```yaml
claude:
  version: "1.0.25"
```

---

### Task 1: Add tempfile dev-dependency

**Files:**
- Modify: `Cargo.toml`

Needed for Task 3's spy backend test.

**Step 1: Add dev dependency**

Add to `Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

**Step 2: Verify**

Run: `just check`
Expected: PASS

**Step 3: Commit**

```
chore: Add tempfile dev-dependency for tests
```

---

### Task 2: Add `claude` config section and test deserialization

**Files:**
- Modify: `src/lib.rs:20-28` (Config struct)
- Modify: `src/lib.rs:289-416` (tests module)

**Step 1: Write the failing tests**

Add to the `tests` module in `src/lib.rs`:

```rust
#[test]
fn config_claude_version() {
    let yaml = r#"
claude:
  version: "1.0.25"
"#;
    let config: Config = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.claude.version.as_deref(), Some("1.0.25"));
}

#[test]
fn config_claude_defaults_to_no_version() {
    let config: Config = serde_yaml_ng::from_str("{}").unwrap();
    assert_eq!(config.claude.version, None);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test config_claude`
Expected: FAIL — `Config` has no field `claude`

**Step 3: Add the ClaudeConfig struct and field**

Add after the `BridgeConfig` impl block (after line 49):

```rust
#[derive(Debug, Default, Deserialize)]
pub struct ClaudeConfig {
    #[serde(default)]
    pub version: Option<String>,
}
```

Add to `Config` struct:

```rust
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
```

**Step 4: Run tests to verify they pass**

Run: `cargo test config_claude`
Expected: PASS

**Step 5: Run full suite**

Run: `just all`
Expected: All checks pass

**Step 6: Commit**

```
feat: Add claude config section with version field
```

---

### Task 3: Add build arg to Dockerfile

**Files:**
- Modify: `image/Dockerfile:20-21` (install line)

**Step 1: Add ARG and use it conditionally**

Change lines 20-21 from:

```dockerfile
# Install Claude Code via native installer
RUN curl -fsSL https://claude.ai/install.sh | bash
```

to:

```dockerfile
# Install Claude Code via native installer
ARG CLAUDE_VERSION=
RUN curl -fsSL https://claude.ai/install.sh | bash${CLAUDE_VERSION:+ -s -- $CLAUDE_VERSION}
```

**Step 2: Run full suite**

Run: `just all`
Expected: All checks pass

**Step 3: Commit**

```
feat: Accept CLAUDE_VERSION build arg in Dockerfile
```

---

### Task 4: Add build args to Backend trait and Docker implementation

**Files:**
- Modify: `src/lib.rs:101-125` (Backend trait and Docker::build)
- Modify: `src/lib.rs:202-228` (Contenant::run build call sites)

**Step 1: Write the failing test**

Add to the `tests` module:

```rust
#[test]
fn build_passes_claude_version_as_build_arg() {
    use std::sync::{Arc, Mutex};

    struct SpyBackend {
        builds: Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>,
    }

    impl Backend for SpyBackend {
        fn build(
            &self,
            tag: &str,
            _path: &Path,
            build_args: &[(&str, &str)],
        ) -> Result<()> {
            self.builds.lock().unwrap().push((
                tag.to_string(),
                build_args
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            Ok(())
        }

        fn tag(&self, _source: &str, _target: &str) -> Result<()> {
            Ok(())
        }

        fn run(
            &self,
            _tag: &str,
            _mounts: &[String],
            _env: &HashMap<String, String>,
        ) -> Result<i32> {
            Ok(0)
        }
    }

    let builds = Arc::new(Mutex::new(Vec::new()));
    let backend = SpyBackend {
        builds: builds.clone(),
    };

    let tmp = tempfile::tempdir().unwrap();
    let app_dirs = xdg::BaseDirectories::with_prefix("contenant-test-build-args");

    let contenant = Contenant {
        backend,
        config: Config {
            claude: ClaudeConfig {
                version: Some("1.0.25".to_string()),
            },
            ..Config::default()
        },
        app_dirs,
        project_dir: tmp.path().to_path_buf(),
    };

    contenant.run().unwrap();

    let builds = builds.lock().unwrap();
    let base_build = builds
        .iter()
        .find(|(tag, _)| tag == "contenant:base")
        .unwrap();
    assert!(base_build
        .1
        .contains(&("CLAUDE_VERSION".to_string(), "1.0.25".to_string())));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test build_passes_claude_version`
Expected: FAIL — `Backend::build` doesn't accept `build_args`

**Step 3: Update the Backend trait**

Change the trait definition:

```rust
pub trait Backend {
    fn build(&self, image: &str, context: &Path, build_args: &[(&str, &str)]) -> Result<()>;
    fn tag(&self, source: &str, target: &str) -> Result<()>;
    fn run(&self, image: &str, mounts: &[String], env: &HashMap<String, String>) -> Result<i32>;
}
```

**Step 4: Update Docker::build**

```rust
fn build(&self, tag: &str, path: &Path, build_args: &[(&str, &str)]) -> Result<()> {
    info!(tag, "Building image");

    let path = path
        .to_str()
        .ok_or_eyre("build context path is not valid UTF-8")?;

    let mut cmd = Command::new("docker");
    cmd.args(["build", "-t", tag]);
    for (key, value) in build_args {
        cmd.args(["--build-arg", &format!("{}={}", key, value)]);
    }
    cmd.arg(path);

    let status = cmd.status()?;

    if !status.success() {
        bail!("Docker build failed");
    }

    Ok(())
}
```

**Step 5: Update all build call sites in `Contenant::run`**

Base image build (line 211) — pass version if configured:
```rust
let build_args: Vec<(&str, &str)> = self
    .config
    .claude
    .version
    .as_deref()
    .map(|v| vec![("CLAUDE_VERSION", v)])
    .unwrap_or_default();
self.backend.build("contenant:base", &context, &build_args)?;
```

User image build (line 217) — pass empty args:
```rust
self.backend.build("contenant:user", context, &[])?;
```

Project image build (line 227) — pass empty args:
```rust
self.backend.build(&run_image, context, &[])?;
```

**Step 6: Run tests to verify they pass**

Run: `cargo test build_passes_claude_version`
Expected: PASS

**Step 7: Run full suite**

Run: `just all`
Expected: All checks pass

**Step 8: Commit**

```
feat: Pass claude version as Docker build arg
```
