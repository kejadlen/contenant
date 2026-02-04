# Shell Autocompletion Support - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a `completions` subcommand that generates shell completion scripts for bash, zsh, and fish.

**Architecture:** Use `clap_complete` to generate completion scripts from the existing clap `Command` definition. Add a hidden `Completions` subcommand that takes a shell name and writes the completion script to stdout.

**Tech Stack:** `clap_complete` crate, clap's `CommandFactory` trait

---

### Task 1: Add clap_complete dependency

**Files:**
- Modify: `Cargo.toml:8`

**Step 1: Add dependency**

Add `clap_complete` to `[dependencies]` in `Cargo.toml`:

```toml
clap_complete = "*"
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles without errors

**Step 3: Commit**

```
feat: Add clap_complete dependency for shell completions
```

---

### Task 2: Add completions subcommand

**Files:**
- Modify: `src/main.rs`

**Step 1: Add the Completions variant to Command enum**

Add to the `Command` enum:

```rust
/// Generate shell completion scripts
#[command(hide = true)]
Completions {
    /// Shell to generate completions for
    shell: clap_complete::Shell,
},
```

**Step 2: Handle the new variant in main()**

Add a match arm:

```rust
Command::Completions { shell } => {
    clap_complete::generate(
        shell,
        &mut Cli::command(),
        "contenant",
        &mut std::io::stdout(),
    );
    Ok(std::process::ExitCode::SUCCESS)
}
```

**Step 3: Run tests and verify**

Run: `just all`
Expected: all checks pass

**Step 4: Manual verification**

Run: `cargo run -- completions bash | head -5`
Expected: outputs bash completion script

Run: `cargo run -- completions zsh | head -5`
Expected: outputs zsh completion script

Run: `cargo run -- completions fish | head -5`
Expected: outputs fish completion script

**Step 5: Commit**

```
feat: Add hidden completions subcommand for bash, zsh, and fish
```

Closes #10.
