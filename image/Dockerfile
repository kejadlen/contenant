FROM debian:trixie-slim

ARG IMAGE_HASH
LABEL contenant.hash=${IMAGE_HASH}

RUN apt-get update && apt-get install -y \
    curl \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install jj (Jujutsu VCS)
# TODO: multi-arch support (currently aarch64 only)
RUN curl -fsSL https://github.com/jj-vcs/jj/releases/download/v0.37.0/jj-v0.37.0-aarch64-unknown-linux-musl.tar.gz \
    | tar -xz -C /usr/local/bin ./jj

# Create non-root user
RUN useradd -m -s /bin/bash claude

USER claude
WORKDIR /home/claude
ENV PATH="/home/claude/.local/bin:$PATH"

# Install Claude Code via native installer (as claude user)
RUN curl -fsSL https://claude.ai/install.sh | bash

# Skip onboarding, trust /project, disable auto-update
COPY --chown=claude:claude claude.json /home/claude/.claude.json

# JJ signing override: use ssh-keygen (talks to forwarded SSH agent) instead of macOS op-ssh-sign
RUN mkdir -p /home/claude/.config/jj/conf.d
COPY --chown=claude:claude jj-container-signing.toml /home/claude/.config/jj/conf.d/00-container-signing.toml

ENTRYPOINT ["claude"]
