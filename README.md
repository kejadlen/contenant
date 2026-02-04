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
  - source: ~/.gitconfig
  - source: /data/shared
    target: /mnt/data
  - source: scripts/setup.sh
    target: /usr/local/bin/setup.sh
    readonly: true

env:
  ANTHROPIC_API_KEY: sk-ant-...
```

### Mounts

`~` expands to the host home in `source` and to the container home (`/home/claude`) in `target`:

```yaml
- source: ~/.ssh
  target: ~/.ssh
  readonly: true
# /home/you/.ssh → /home/claude/.ssh (read-only)
```

When `target` is omitted it defaults to `source`, with `~` expanded for the container:

```yaml
- source: ~/.gitconfig
# /home/you/.gitconfig → /home/claude/.gitconfig
```

Absolute paths are passed through as-is:

```yaml
- source: /data/shared
  target: /mnt/data
# /data/shared → /mnt/data
```

Relative source paths resolve from the config directory (`~/.config/contenant/`):

```yaml
- source: scripts/setup.sh
  target: /usr/local/bin/setup.sh
# ~/.config/contenant/scripts/setup.sh → /usr/local/bin/setup.sh
```

Mounts are readonly by default; set `readonly: false` for read-write access.

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

## Skills

If `~/.config/contenant/skills/` exists, it is automatically mounted to `~/.claude/skills/` inside the container. This allows you to share Claude Code skills between the host and container.

## Bridge and Triggers

The bridge is a host-side HTTP server that allows Claude Code running inside the container to execute predefined commands on the host machine. This enables workflows like opening files in your editor or sending notifications.

### Configuration

Add a `bridge` section to `~/.config/contenant/config.yml`:

```yaml
bridge:
  port: 19432  # optional, this is the default
  triggers:
    open-editor: "code ."
    notify: "notify-send 'Task completed'"
    open-browser: "xdg-open https://example.com"
```

### Starting the Bridge

Run the bridge server in a separate terminal before starting the container:

```bash
contenant bridge
```

### Using Triggers from the Container

Inside the container, the `CONTENANT_BRIDGE_URL` environment variable points to the bridge server. Claude Code (or any process in the container) can invoke triggers via HTTP:

```bash
curl -X POST "$CONTENANT_BRIDGE_URL/triggers/open-editor"
```

The response includes the command's exit code, stdout, and stderr:

```json
{
  "exit_code": 0,
  "stdout": "",
  "stderr": ""
}
```

### Security Note

Triggers execute shell commands on your host machine. Only define triggers you trust and be mindful of what commands you expose.

## Shell Completions

Add to your shell configuration:

```bash
# bash (~/.bashrc)
source <(COMPLETE=bash contenant)

# zsh (~/.zshrc)
source <(COMPLETE=zsh contenant)

# fish (~/.config/fish/config.fish)
COMPLETE=fish contenant | source
```

Completions include claude's own flags after `--` in the `run` subcommand.
