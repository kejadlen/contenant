# CLAUDE.md

## Build Commands

```bash
just all      # Run fmt, check, clippy, test
just fmt      # Format code
just check    # Check compilation
just test     # Run tests
just clippy   # Run clippy with -D warnings
```

Run a single test: `cargo test <test_name>`

Enable tracing: `RUST_LOG=debug cargo run`

## CLI Usage

```
contenant [run [PATH] [-- CLAUDE_ARGS...]]   # Run claude in container (default: run .)
contenant bridge                              # Start host command bridge server
contenant completions <SHELL>                 # Generate shell completions (hidden)
```

If no subcommand is given, `run .` is assumed.

## Architecture

Contenant runs Claude Code inside Docker containers with persistent state and configurable mounts.

**Core flow:** `main.rs` parses CLI args (clap) and delegates to `Contenant::run()` in `lib.rs`, which:
1. Writes embedded Dockerfile and claude.json from `assets/` to XDG cache
2. Builds base image (`contenant:base`)
3. Optionally builds user image (`contenant:user`) if user provides `~/.config/contenant/Dockerfile`
4. Optionally builds project image (`contenant:<project-id>`) if `.contenant/Dockerfile` exists in project root
5. Mounts persistent state, user mounts, and env vars
6. Runs container with workspace at `/workspace`, returns container exit code

**Backend trait:** `Backend` (build/tag/run) abstracts container operations. Only `Docker` implements it currently.

**Embedded files:** Files in `assets/` are compiled into the binary via `include_str!`.

**Project isolation:** `project_id()` produces `<8-char-sha256>-<dirname>` from the canonical project path.

### Bridge Server

`contenant bridge` starts an HTTP server (default port 19432) that exposes named triggers as `POST /triggers/{name}`. Triggers execute shell commands on the host and return `{ exit_code, stdout, stderr }`. The container receives `CONTENANT_BRIDGE_URL=http://host.docker.internal:<port>` automatically.

Implementation: `src/bridge.rs` (axum + tokio).

### Persistent Mounts (automatic)

| Host path | Container path | Purpose |
|-----------|---------------|---------|
| `~/.local/share/contenant/claude/` | `/home/claude/.claude` | Claude auth & settings |
| `~/.config/contenant/skills/` (if exists) | `/home/claude/.claude/skills` | Shared skills |
| `~/.local/share/contenant/ssh/known_hosts` | `/home/claude/.ssh/known_hosts` | SSH host keys |

User-defined mounts (from config) are appended after these and can shadow subdirectories.

### Layered Config (`StackedConfig`)

Configuration uses a layered architecture inspired by jj's `StackedConfig`. Each layer is a `(ConfigSource, Config)` pair stored in precedence order. Layers are preserved individually — accessors resolve across them on read.

**Current layers (lowest → highest precedence):**
- `User` — `~/.config/contenant/config.yml`

**Resolution rules per field:**
- `claude.version`, `allowed_domains` — last layer to set wins
- `mounts` — accumulated across all layers (lowest precedence first)
- `env`, `bridge.triggers` — merged; higher precedence overrides per-key
- `bridge.port` — last non-default value wins

### Config Schema (`~/.config/contenant/config.yml`)

```yaml
claude:
  version: "..."          # Optional: CLAUDE_VERSION build arg

mounts:                    # Additional volume mounts
  - source: ~/path         # ~ expands to $HOME on host, /home/claude in target
    target: ~/dest         # Optional: defaults to source path
    readonly: true         # Default: true

env:                       # Extra env vars passed to container
  KEY: value

bridge:
  port: 19432              # Default: 19432
  triggers:
    name: "shell command"  # Named commands callable via HTTP POST
```

Mount sources support `~` expansion (host `$HOME`) and relative paths (resolved from config dir). Mount targets expand `~` to `/home/claude`.

## Gotchas

- Container reaches host via `--add-host host.docker.internal:host-gateway` (Docker networking)
- Container exit code is passed through as the process exit code; signal termination is an error
- Error handling uses `color_eyre`
- All dependency versions in Cargo.toml are unconstrained (`*`)

## Verification

Run `just all` after completing each chunk of work.
