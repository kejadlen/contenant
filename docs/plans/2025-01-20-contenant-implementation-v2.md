# contenant Implementation Plan v2

> Restructured for incremental testability. Each phase produces a runnable contenant.

**Goal:** Build a CLI that runs Claude Code in isolated Linux containers using Apple's `container` tool.

---

## Completed Phases

### Phase 1: Working Image ✓

Built OCI image with Claude Code installed.

### Phase 2: Minimal CLI ✓

Basic CLI that runs `container run` with project mounted at `/project`.

### Phase 2.5: Command Passthrough ✓

Support running custom commands: `contenant -- bash`, `contenant -- ls /project`

### Phase 2.6: Auth Sharing ✓

- Extract OAuth token from macOS keychain
- Pass as `CLAUDE_CODE_OAUTH_TOKEN` to container
- Bake `hasCompletedOnboarding` and `/project` trust into image

### Phase 2.7: Environment Customization (TODO - from host)

**Goal:** Customize container environment with preferred tools and Claude settings.

**Testable outcome:** Container has jj installed and personal Claude customizations applied.

**NOTE:** This phase must be executed from the HOST machine (outside container).

**Changes:**
1. Install jj (Jujutsu VCS) in Dockerfile
   - Download and install from GitHub releases or use package manager
2. Add personal Claude customizations to container
   - Custom prompts/skills from host ~/.claude directory
   - Copy into image during build or mount at runtime
3. Rebuild and test image from host

**Implementation notes:**
- Need to access host's ~/.claude directory for custom prompts/skills
- Dockerfile modifications must be done from host
- Image rebuild requires Docker/container CLI access from host

---

## Remaining Phases

### Phase 3: Per-Project Containers ✓

**Goal:** Each project gets its own named, reusable container.

**Testable outcome:** Running `contenant` twice in the same directory reattaches to the same container.

**Changes:**
1. Add `sha2` dependency ✓
2. Generate container ID from project path hash: `contenant-{basename}-{short-hash}` ✓
3. Check if container exists (inspect returns success for existing containers) ✓
4. If exists: `container start -ai <id>` ✓
5. If not: `container run --name <id> ...` (remove `--rm`) ✓

### Phase 4: List and Clean Commands

**Goal:** Manage containers.

**Testable outcome:** `contenant list` shows containers, `contenant clean [path]` removes them.

**Changes:**
1. Add `clap` dependency
2. Add subcommands: `list`, `clean [path]`, `clean --all`
3. Filter `container list -a` output for `contenant-*` prefix

### Phase 5: Firewall

**Goal:** Network isolation with allowlist-only outbound.

**Testable outcome:** Container can only reach Anthropic APIs + DNS.

**Changes:**
1. Add `entrypoint.sh` with iptables rules
2. Update Dockerfile: add iptables, sudo, entrypoint
3. Allow: loopback, established, DNS (udp 53), api.anthropic.com, statsig.anthropic.com, sentry.io
4. Drop everything else

---

## Summary

| Phase | Status | Description |
|-------|--------|-------------|
| 1 | ✓ | Container image with Claude Code |
| 2 | ✓ | Minimal CLI that runs container |
| 2.5 | ✓ | Command passthrough |
| 2.6 | ✓ | Auth sharing from host keychain |
| 2.7 | ✓ | Environment customization (jj, skills, commands) |
| 3 | ✓ | Per-project container identity |
| 4 | TODO | List and clean commands |
| 5 | TODO | Network firewall |
