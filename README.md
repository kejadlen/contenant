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

Create `~/.config/contenant/config.yml` to define additional mounts and environment variables:

```yaml
mounts:
  - source: ~/.ssh
    target: ~/.ssh
    readonly: true
  - source: ~/.gitconfig
    target: ~/.gitconfig
    readonly: true

env:
  ANTHROPIC_API_KEY: sk-ant-...
```

### Mounts

- `~` in `source` expands to the host home directory
- `~` in `target` expands to the container home (`/home/claude`)
- `target` is optional and defaults to `source` (with tilde expanded for the container)
- Relative source paths resolve from the config directory (`~/.config/contenant/`)

### Environment Variables

The `env` map passes environment variables into the container. Values support tilde expansion to the container home directory.

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
