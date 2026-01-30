# Network Firewall Design

Restrict outbound network access from Contenant containers to an allowlist of approved domains.

## Approach

Split the work between the host (Rust binary) and the container (entrypoint script). The host resolves domain names to IP addresses and writes them to a file. The container reads that file and configures iptables to block all other traffic.

## Host Side (Rust)

### Configuration

Add an optional `allowed_domains` field to `Config`:

```yaml
# ~/.config/contenant/config.yml
allowed_domains:
  - api.github.com
  - github.com
  - registry.npmjs.org
  - api.anthropic.com
```

If absent, use a hardcoded default list:
- `api.github.com`, `github.com`
- `api.anthropic.com`

If present, the user's list fully replaces the defaults.

### IP Resolution

A new function resolves the allowed domains into IPs and CIDRs:

1. For each domain, resolve A records via `hickory-resolver` and collect IPs.
2. If `api.github.com` is in the list, fetch `https://api.github.com/meta` via `ureq` and extract the `.web`, `.api`, and `.git` CIDR arrays.
3. Write all IPs and CIDRs to a temp file, one per line.

### Container Wiring

In `Contenant::run()`, after building images and before calling `backend.run()`:

1. Read `allowed_domains` from config (or use defaults).
2. Resolve domains to an IP file.
3. Add a read-only mount: `<temp_file>:/etc/contenant/allowed-ips:ro`.

No changes to the `Backend` trait or `Backend::run()` signature needed. The IP file mount is just another entry in the existing `mounts` slice.

### New Crates

- `ureq` -- blocking HTTP client for the GitHub meta API
- `hickory-resolver` -- DNS resolution

## Container Side

### Dockerfile Changes

- Add `iptables` and `ipset` to the `apt-get install` line.
- Remove `USER claude` (the entrypoint runs as root and drops privileges).
- Replace `ENTRYPOINT ["claude"]` with the new entrypoint script.

### Entrypoint Script

A new embedded file `image/entrypoint.sh` runs as root at container start:

1. Preserve Docker DNS NAT rules before flushing iptables.
2. Flush all iptables rules and destroy existing ipsets.
3. Restore Docker DNS rules.
4. Allow DNS (port 53), SSH (port 22), and localhost.
5. Create an ipset `allowed-domains` and populate it from `/etc/contenant/allowed-ips`.
6. Detect the host network from the default route and allow it.
7. Set default DROP policy on INPUT, FORWARD, and OUTPUT.
8. Allow established/related connections and traffic to the ipset.
9. REJECT all other outbound traffic (immediate feedback, not timeout).
10. Drop privileges and exec: `su -s /bin/bash claude -c "claude $*"`.

### Embedded in Binary

The entrypoint script is compiled into the binary via `include_str!`, matching the existing pattern for `Dockerfile` and `claude.json`. It gets written to the build context alongside them.

## Files Changed

| File | Change |
|------|--------|
| `Cargo.toml` | Add `ureq`, `hickory-resolver` |
| `src/lib.rs` | Add `allowed_domains` to Config, add resolve function, mount IP file |
| `image/Dockerfile` | Add `iptables`/`ipset`, use entrypoint script |
| `image/entrypoint.sh` | New embedded file |

## Verification

Manual integration test: run `contenant run`, then inside the container confirm that `curl https://api.anthropic.com` succeeds and `curl https://example.com` fails immediately.
