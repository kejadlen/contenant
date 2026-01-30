# Network Firewall Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Restrict container outbound network access to an allowlist of approved domains.

**Architecture:** The host resolves allowed domain names to IPs and writes them to a file. The container reads that file at startup and configures iptables to drop all other outbound traffic. See `docs/plans/2026-01-30-network-firewall-design.md` for the full design.

**Tech Stack:** Rust (`ureq`, `hickory-resolver`, `serde_json`, `tempfile`), iptables/ipset in the container, embedded shell entrypoint script.

---

### Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add new crates**

Add four new dependencies to `Cargo.toml` under `[dependencies]`:

```toml
ureq = "*"
hickory-resolver = "*"
serde_json = "*"
tempfile = "*"
```

- `ureq` — blocking HTTP client for fetching GitHub IP ranges
- `hickory-resolver` — DNS resolution on the host
- `serde_json` — parse the GitHub `/meta` API response
- `tempfile` — create the IP file that persists until the container exits

**Step 2: Verify compilation**

Run: `cargo check`
Expected: compiles with no errors (warnings are fine at this stage)

**Step 3: Commit**

```
feat: Add firewall resolution dependencies
```

---

### Task 2: Add `allowed_domains` to Config

**Files:**
- Modify: `src/lib.rs:14-20` (Config struct)

**Step 1: Add the field**

Add `allowed_domains` to the `Config` struct:

```rust
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub mounts: Vec<Mount>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub allowed_domains: Option<Vec<String>>,
}
```

No `#[serde(default)]` needed — `Option` already deserializes as `None` when absent.

**Step 2: Add default domains constant**

Add near the top of `lib.rs`, after the existing constants:

```rust
const DEFAULT_ALLOWED_DOMAINS: &[&str] = &[
    "api.github.com",
    "github.com",
    "api.anthropic.com",
];
```

**Step 3: Add a helper method on Config**

```rust
impl Config {
    pub fn allowed_domains(&self) -> Vec<String> {
        match &self.allowed_domains {
            Some(domains) => domains.clone(),
            None => DEFAULT_ALLOWED_DOMAINS.iter().map(|s| s.to_string()).collect(),
        }
    }

    // existing load() method stays unchanged
}
```

**Step 4: Verify compilation**

Run: `just all`
Expected: passes (fmt, check, clippy, test)

**Step 5: Commit**

```
feat: Add allowed_domains config field

Defaults to GitHub and Anthropic API when absent.
User-provided list fully replaces defaults.
```

---

### Task 3: Implement IP resolution

**Files:**
- Modify: `src/lib.rs` (add new function)

**Step 1: Add the resolve function**

Add a standalone function in `lib.rs` (before the `Contenant` struct):

```rust
use std::io::Write;
use std::net::IpAddr;

use hickory_resolver::Resolver;
use tempfile::NamedTempFile;

/// Resolve allowed domains to IPs/CIDRs and write them to a temp file.
///
/// The returned `NamedTempFile` must outlive the container process — dropping
/// it deletes the file. The caller should hold onto it until `backend.run()`
/// returns.
fn resolve_allowed_ips(domains: &[String]) -> Result<NamedTempFile> {
    let resolver = Resolver::default();
    let mut file = NamedTempFile::new()?;

    // If api.github.com is in the list, fetch GitHub's published CIDR ranges
    if domains.iter().any(|d| d == "api.github.com") {
        info!("Fetching GitHub IP ranges");
        let body: serde_json::Value = ureq::get("https://api.github.com/meta")
            .call()?
            .body_mut()
            .read_json()?;

        for key in &["web", "api", "git"] {
            if let Some(ranges) = body[key].as_array() {
                for range in ranges {
                    if let Some(cidr) = range.as_str() {
                        // Only include IPv4 CIDRs
                        if cidr.contains('.') {
                            info!(cidr, "Adding GitHub range");
                            writeln!(file, "{}", cidr)?;
                        }
                    }
                }
            }
        }
    }

    // Resolve each domain to A records
    for domain in domains {
        info!(domain, "Resolving domain");
        match resolver.lookup_ip(domain) {
            Ok(response) => {
                for ip in response.iter() {
                    if let IpAddr::V4(v4) = ip {
                        let entry = format!("{}/32", v4);
                        info!(entry, domain, "Adding IP");
                        writeln!(file, "{}", entry)?;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(domain, error = %e, "Failed to resolve domain");
            }
        }
    }

    file.flush()?;
    Ok(file)
}
```

Key decisions:
- Returns `NamedTempFile` so the caller controls its lifetime. The file is deleted on drop, which must happen after `backend.run()` returns.
- IPv4 only — ipset `hash:net` with IPv4 CIDRs. IPv6 support can be added later.
- GitHub resolution failure is a warning, not an error. The container still starts with whatever IPs resolved successfully.
- Each plain IP gets a `/32` suffix so ipset treats everything uniformly as CIDR.

**Step 2: Verify compilation**

Run: `cargo check`
Expected: compiles (the function isn't called yet, so clippy may warn about dead code — that's fine)

**Step 3: Commit**

```
feat: Resolve allowed domains to IP file

Fetches GitHub CIDR ranges from their meta API and
resolves other domains via DNS. Writes one CIDR per
line for the container entrypoint to consume.
```

---

### Task 4: Wire resolution into Contenant::run()

**Files:**
- Modify: `src/lib.rs:168-227` (Contenant::run method)

**Step 1: Call resolve and mount the file**

In the `run()` method, after building images and before calling `backend.run()`, add the resolution step. The final section of `run()` should look like:

```rust
    pub fn run(&self) -> Result<i32> {
        // ... existing image build code (lines 170-194) stays unchanged ...

        // Default mount: persist Claude state (auth, settings, etc.)
        let claude_state_dir = self.app_dirs.place_state_file("claude")?;
        fs::create_dir_all(&claude_state_dir)?;
        let mut mounts = vec![format!(
            "{}:{}/.claude",
            claude_state_dir.display(),
            CONTAINER_HOME
        )];

        // User-defined mounts (can shadow subdirectories of defaults)
        let config_dir = self.app_dirs.get_config_home().unwrap();
        let user_mounts: Vec<_> = self
            .config
            .mounts
            .iter()
            .map(|mount| mount.to_docker_volume(&config_dir))
            .collect();
        mounts.extend(user_mounts);

        // Resolve allowed domains and mount the IP file into the container
        let domains = self.config.allowed_domains();
        let allowed_ips_file = resolve_allowed_ips(&domains)?;
        mounts.push(format!(
            "{}:/etc/contenant/allowed-ips:ro",
            allowed_ips_file.path().display()
        ));

        let env: HashMap<_, _> = self
            .config
            .env
            .iter()
            .map(|(key, value)| {
                let value =
                    shellexpand::tilde_with_context(value, || Some(CONTAINER_HOME.to_string()));
                (key.clone(), value.into_owned())
            })
            .collect();

        self.backend.run(&run_image, &mounts, &env)
    }
```

The `allowed_ips_file` binding keeps the `NamedTempFile` alive until `backend.run()` returns, then drops it (deleting the temp file).

**Step 2: Verify compilation**

Run: `just all`
Expected: passes

**Step 3: Commit**

```
feat: Mount resolved IPs into container

Resolves allowed domains before container start and
mounts the result at /etc/contenant/allowed-ips.
```

---

### Task 5: Create the entrypoint script

**Files:**
- Create: `image/entrypoint.sh`

**Step 1: Write the script**

Create `image/entrypoint.sh`:

```bash
#!/bin/bash
set -euo pipefail
IFS=$'\n\t'

# Preserve Docker DNS NAT rules before flushing
DOCKER_DNS_RULES=$(iptables-save -t nat | grep "127\.0\.0\.11" || true)

# Flush all existing rules
iptables -F
iptables -X
iptables -t nat -F
iptables -t nat -X
iptables -t mangle -F
iptables -t mangle -X
ipset destroy allowed-domains 2>/dev/null || true

# Restore Docker DNS resolution
if [ -n "$DOCKER_DNS_RULES" ]; then
    iptables -t nat -N DOCKER_OUTPUT 2>/dev/null || true
    iptables -t nat -N DOCKER_POSTROUTING 2>/dev/null || true
    echo "$DOCKER_DNS_RULES" | xargs -L 1 iptables -t nat
fi

# Allow DNS, SSH, and localhost before any restrictions
iptables -A OUTPUT -p udp --dport 53 -j ACCEPT
iptables -A INPUT -p udp --sport 53 -j ACCEPT
iptables -A OUTPUT -p tcp --dport 22 -j ACCEPT
iptables -A INPUT -p tcp --sport 22 -m state --state ESTABLISHED -j ACCEPT
iptables -A INPUT -i lo -j ACCEPT
iptables -A OUTPUT -o lo -j ACCEPT

# Load allowed IPs from the file mounted by contenant
ipset create allowed-domains hash:net
while IFS= read -r cidr; do
    [ -n "$cidr" ] && ipset add allowed-domains "$cidr"
done < /etc/contenant/allowed-ips

# Allow host network (for Docker communication)
HOST_IP=$(ip route | grep default | cut -d" " -f3)
HOST_NETWORK=$(echo "$HOST_IP" | sed "s/\.[0-9]*$/.0\/24/")
iptables -A INPUT -s "$HOST_NETWORK" -j ACCEPT
iptables -A OUTPUT -d "$HOST_NETWORK" -j ACCEPT

# Default policy: drop everything
iptables -P INPUT DROP
iptables -P FORWARD DROP
iptables -P OUTPUT DROP

# Allow established connections (for traffic already approved above)
iptables -A INPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT

# Allow outbound traffic only to allowlisted IPs
iptables -A OUTPUT -m set --match-set allowed-domains dst -j ACCEPT

# Reject everything else with immediate feedback
iptables -A OUTPUT -j REJECT --reject-with icmp-admin-prohibited

# Drop privileges and run Claude Code
exec su -s /bin/bash claude -c "claude $*"
```

**Step 2: Make it executable**

Run: `chmod +x image/entrypoint.sh`

**Step 3: Commit**

```
feat: Add container entrypoint with iptables firewall

Reads pre-resolved IPs from /etc/contenant/allowed-ips,
configures iptables to drop all other outbound traffic,
then drops to the claude user.
```

---

### Task 6: Update the Dockerfile

**Files:**
- Modify: `image/Dockerfile`

**Step 1: Add firewall packages, embed entrypoint, adjust user handling**

Replace the entire Dockerfile with:

```dockerfile
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    ca-certificates \
    iptables \
    ipset \
    iproute2 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash claude

# Install Claude Code as claude user
USER claude
WORKDIR /home/claude
ENV PATH="/home/claude/.local/bin:$PATH"
RUN curl -fsSL https://claude.ai/install.sh | bash

# Pre-configure Claude to skip onboarding and trust /workspace
COPY claude.json /home/claude/.claude.json

# Entrypoint runs as root to configure firewall, then drops to claude
USER root
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
```

Changes from the original:
- Added `iptables`, `ipset`, `iproute2` (for `ip route` in entrypoint) to `apt-get install`
- Claude Code installation still happens as `claude` user
- Switches back to `USER root` at the end so the entrypoint can configure iptables
- Copies and runs `entrypoint.sh` instead of `claude` directly

**Step 2: Embed the entrypoint in the binary**

In `src/lib.rs`, add a new constant next to the existing ones:

```rust
const DOCKERFILE: &str = include_str!("../image/Dockerfile");
const CLAUDE_JSON: &str = include_str!("../image/claude.json");
const ENTRYPOINT: &str = include_str!("../image/entrypoint.sh");
```

**Step 3: Write the entrypoint to the build context**

In `Contenant::run()`, where the Dockerfile and claude.json are written to the cache directory, add:

```rust
let entrypoint_path = self.app_dirs.place_cache_file("entrypoint.sh")?;
fs::write(&entrypoint_path, ENTRYPOINT)?;
```

This goes right after the existing `fs::write(&claude_json_path, CLAUDE_JSON)?;` line.

**Step 4: Verify compilation**

Run: `just all`
Expected: passes

**Step 5: Commit**

```
feat: Embed firewall entrypoint in Dockerfile

The entrypoint runs as root to set up iptables,
then drops to the claude user. Adds iptables,
ipset, and iproute2 to the base image.
```

---

### Task 7: Verify end-to-end

**Step 1: Build and run**

Run: `cargo run`

Expected: Container starts, Claude Code launches. No visible difference in normal operation.

**Step 2: Test firewall inside container**

In a separate terminal, exec into the running container:

```bash
docker exec -it $(docker ps -q --filter ancestor=contenant:base) bash
```

Then test:

```bash
# Should succeed (in allowlist)
curl --connect-timeout 5 https://api.github.com/zen

# Should fail immediately with "connection refused" (not timeout)
curl --connect-timeout 5 https://example.com
```

**Step 3: Commit any fixes**

If adjustments were needed, commit them with a descriptive message.

---

Plan complete and saved to `docs/plans/2026-01-30-network-firewall-plan.md`. Two execution options:

**1. Subagent-Driven (this session)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** — Open a new session with executing-plans, batch execution with checkpoints

Which approach?