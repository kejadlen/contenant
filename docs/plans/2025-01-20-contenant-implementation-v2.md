# contenant Implementation Plan v2

> Restructured for incremental testability. Each phase produces a runnable contenant.

**Goal:** Build a CLI that runs Claude Code in isolated Linux containers using Apple's `container` tool.

---

## Phase 1: Working Image

**Goal:** Build an OCI image that can run Claude.

**Testable outcome:** `container run <image>` launches Claude interactively.

**Files:**
- Create: `image/Dockerfile`

**Steps:**

1. Create `image/Dockerfile`:

```dockerfile
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    curl \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 20 LTS
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Claude Code
RUN npm install -g @anthropic-ai/claude-code

# Create non-root user
RUN useradd -m -s /bin/bash claude
USER claude
WORKDIR /home/claude

ENTRYPOINT ["claude"]
```

2. Build locally:
```bash
docker build -t contenant:latest image/
```

3. Test:
```bash
docker run -it contenant:latest --version
```

4. Commit: `feat: container image with Claude Code`

---

## Phase 2: Minimal CLI That Runs

**Goal:** A `contenant` binary that launches Claude in a container for the current directory.

**Testable outcome:** `cargo run` drops you into Claude inside a container with your project mounted at `/project`.

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/config.rs`

**Steps:**

1. Create `Cargo.toml`:

```toml
[package]
name = "contenant"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
```

2. Create `src/config.rs`:

```rust
use std::path::PathBuf;

pub const IMAGE: &str = "contenant:latest";

pub fn claude_state_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("contenant/claude")
}
```

Note: Add `dirs = "5"` to Cargo.toml dependencies.

3. Create `src/main.rs`:

```rust
mod config;

use anyhow::{Context, Result};
use std::process::Command;

fn main() -> Result<()> {
    let project_path = std::env::current_dir()
        .context("Failed to get current directory")?;

    // Ensure claude state dir exists
    std::fs::create_dir_all(config::claude_state_dir())?;

    let project_mount = format!(
        "type=bind,src={},dst=/project",
        project_path.display()
    );

    let claude_mount = format!(
        "type=bind,src={},dst=/home/claude/.claude",
        config::claude_state_dir().display()
    );

    let status = Command::new("container")
        .args([
            "run",
            "-it",
            "--rm",
            "--workdir", "/project",
            "--mount", &project_mount,
            "--mount", &claude_mount,
            config::IMAGE,
        ])
        .status()
        .context("Failed to run container. Is Apple's container tool installed?")?;

    if !status.success() {
        anyhow::bail!("Container exited with error");
    }

    Ok(())
}
```

4. Verify: `cargo build && cargo run`

5. Commit: `feat: minimal CLI that runs Claude in container`

---

## Phase 3: Per-Project Containers

**Goal:** Each project gets its own named, reusable container.

**Testable outcome:** Running `contenant` twice in the same directory reattaches to the same container. Different directories get different containers.

**Files:**
- Create: `src/container/mod.rs`
- Create: `src/container/id.rs`
- Modify: `src/main.rs`

**Steps:**

1. Add to `Cargo.toml`:

```toml
sha2 = "0.10"
hex = "0.4"
```

2. Create `src/container/mod.rs`:

```rust
pub mod id;
```

3. Create `src/container/id.rs`:

```rust
use sha2::{Digest, Sha256};
use std::path::Path;

/// Generate container ID from project path.
/// Format: contenant-{basename}-{short-hash}
pub fn container_id(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path_str = canonical.to_string_lossy();

    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    let hash = hasher.finalize();
    let short_hash = hex::encode(&hash[..4]);

    let basename = canonical
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "root".to_string());

    format!("contenant-{}-{}", basename, short_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_container_id_format() {
        let path = PathBuf::from("/tmp/myproject");
        let id = container_id(&path);
        assert!(id.starts_with("contenant-myproject-"));
        assert_eq!(id.len(), "contenant-myproject-".len() + 8);
    }

    #[test]
    fn test_container_id_deterministic() {
        let path = PathBuf::from("/tmp/myproject");
        assert_eq!(container_id(&path), container_id(&path));
    }
}
```

4. Update `src/main.rs` to:
   - Generate container ID from project path
   - Check if container exists (`container inspect <id>`)
   - If exists: `container start -a <id>`
   - If not: `container run --name <id> ...` (without `--rm`)

5. Verify: Run twice in same dir, should reattach.

6. Commit: `feat: per-project container identity`

---

## Phase 4: List and Clean Commands

**Goal:** Manage containers.

**Testable outcome:** `contenant list` shows containers, `contenant clean [path]` removes them.

**Files:**
- Create: `src/cli.rs`
- Create: `src/container/manager.rs`
- Modify: `src/container/mod.rs`
- Modify: `src/main.rs`

**Steps:**

1. Add to `Cargo.toml`:

```toml
clap = { version = "4", features = ["derive"] }
```

2. Create `src/cli.rs`:

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "contenant")]
#[command(about = "Run Claude Code in isolated containers")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// List all project containers
    List,

    /// Remove container(s)
    Clean {
        /// Project path (defaults to current directory)
        path: Option<PathBuf>,

        /// Remove all containers
        #[arg(long)]
        all: bool,
    },
}
```

3. Create `src/container/manager.rs`:

```rust
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use super::id::container_id;

pub fn list() -> Result<Vec<String>> {
    let output = Command::new("container")
        .args(["list", "-a", "--format", "{{.Names}}"])
        .output()
        .context("Failed to run 'container list'")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let containers: Vec<String> = stdout
        .lines()
        .filter(|line| line.starts_with("contenant-"))
        .map(|s| s.to_string())
        .collect();

    Ok(containers)
}

pub fn clean(path: &Path) -> Result<()> {
    let id = container_id(path);
    remove_container(&id)
}

pub fn clean_all() -> Result<()> {
    for name in list()? {
        remove_container(&name)?;
    }
    Ok(())
}

fn remove_container(name: &str) -> Result<()> {
    let _ = Command::new("container").args(["stop", name]).output();
    let output = Command::new("container")
        .args(["rm", name])
        .output()
        .context("Failed to remove container")?;

    if output.status.success() {
        println!("Removed: {}", name);
    }
    Ok(())
}
```

4. Update `src/main.rs` to dispatch based on CLI args.

5. Verify:
   - `cargo run -- list`
   - `cargo run -- clean`
   - `cargo run -- clean --all`
   - `cargo run -- clean /some/path`

6. Commit: `feat: list and clean commands`

---

## Phase 5: Firewall

**Goal:** Network isolation with allowlist-only outbound.

**Testable outcome:** Container can only reach Anthropic APIs + DNS.

**Files:**
- Create: `image/entrypoint.sh`
- Modify: `image/Dockerfile`

**Steps:**

1. Create `image/entrypoint.sh`:

```bash
#!/bin/bash
set -e

# Apply firewall rules
sudo iptables -F OUTPUT 2>/dev/null || true
sudo iptables -A OUTPUT -o lo -j ACCEPT
sudo iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
sudo iptables -A OUTPUT -p udp --dport 53 -j ACCEPT
sudo iptables -A OUTPUT -p tcp -d api.anthropic.com --dport 443 -j ACCEPT
sudo iptables -A OUTPUT -p tcp -d statsig.anthropic.com --dport 443 -j ACCEPT
sudo iptables -A OUTPUT -p tcp -d sentry.io --dport 443 -j ACCEPT
sudo iptables -A OUTPUT -j DROP

exec "$@"
```

2. Update `image/Dockerfile`:

```dockerfile
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    curl \
    git \
    ca-certificates \
    iptables \
    sudo \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @anthropic-ai/claude-code

RUN useradd -m -s /bin/bash claude \
    && echo "claude ALL=(ALL) NOPASSWD: /sbin/iptables" >> /etc/sudoers

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /project
USER claude

ENTRYPOINT ["/entrypoint.sh"]
CMD ["claude"]
```

3. Rebuild image: `docker build -t contenant:latest image/`

4. Verify: From inside container, `curl https://example.com` should fail, `curl https://api.anthropic.com` should work.

5. Commit: `feat: network firewall with allowlist`

---

## Summary

| Phase | Commit | Test |
|-------|--------|------|
| 1 | `feat: container image with Claude Code` | `docker run -it contenant:latest --version` |
| 2 | `feat: minimal CLI that runs Claude in container` | `cargo run` launches Claude |
| 3 | `feat: per-project container identity` | Run twice, reattaches |
| 4 | `feat: list and clean commands` | `contenant list`, `contenant clean` |
| 5 | `feat: network firewall with allowlist` | Outbound blocked except allowlist |
