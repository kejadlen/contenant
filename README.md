# contenant

Run Claude Code inside Docker containers with persistent state and configurable mounts.

## Installation

```bash
cargo install --path .
```

## Usage

Run from any project directory:

```bash
contenant
```

Or specify a project directory:

```bash
contenant run /path/to/project
```

This mounts the project directory at `/workspace` inside the container and starts Claude Code.

Enable debug logging with `RUST_LOG=debug contenant`.

## Configuration

Create `~/.config/contenant/config.yml` to define additional mounts:

```yaml
mounts:
  - source: $HOME/.ssh
    target: /home/claude/.ssh
    readonly: true
  - source: $HOME/.gitconfig
    target: /home/claude/.gitconfig
    readonly: true
```

Supported variables: `$HOME`, `$CONTENANT_CONFIG_DIR`, `$CONTENANT_CONTAINER_HOME`, and any environment variable.

## Image Layering

Contenant builds images in layers:

1. **contenant:base** - Debian with Claude Code installed
2. **contenant:user** - Your customizations from `~/.config/contenant/Dockerfile` (or base if none)
3. **contenant:project** - Project-specific from `.contenant/Dockerfile` (optional)

User Dockerfile example:

```dockerfile
FROM contenant:base

RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs
```

Project Dockerfile example:

```dockerfile
FROM contenant:user

RUN cargo install cargo-watch
```

## State Persistence

Claude authentication and settings persist across runs in `~/.local/state/contenant/claude/`.

Each project gets isolated XDG directories based on its path hash.
