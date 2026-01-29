# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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

## Architecture

Contenant runs Claude Code inside Docker containers with persistent state and configurable mounts.

**Core flow:** `main.rs` parses CLI args and delegates to `Contenant::run()` in `lib.rs`, which:
1. Writes embedded Dockerfile and claude.json from `image/` to XDG cache
2. Builds base image (`contenant:base`)
3. Optionally builds user image (`contenant:user`) if user provides `~/.config/contenant/Dockerfile`
4. Optionally builds project image (`contenant:<project-id>`) if `.contenant/Dockerfile` exists in project root
5. Runs container with workspace mounted at `/workspace` and Claude state persisted

**Backend trait:** `Backend` abstracts container operations (build/run). Currently only `Docker` implements it, but the design allows swapping runtimes.

**Configuration:** `Config` loaded from `~/.config/contenant/config.yml` defines additional mounts with shell expansion support (`$HOME`, `$CONTENANT_CONFIG_DIR`, etc.).

**Project isolation:** Each project gets unique XDG directories via `project_id()` which hashes the canonical project path.

**Embedded files:** `image/Dockerfile` and `image/claude.json` are compiled into the binary via `include_str!`.

## Verification

Run `just all` after completing each chunk of work.

## Maintaining This File

Update CLAUDE.md when:
- Build commands or workflows change
- New architectural patterns or abstractions are introduced
- Configuration options are added or modified
