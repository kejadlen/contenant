FROM debian:trixie-slim

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

# Install Claude Code via native installer (as claude user)
RUN curl -fsSL https://claude.ai/install.sh | bash

# Skip onboarding, trust /project, disable auto-update
RUN echo '{"hasCompletedOnboarding":true,"autoUpdaterStatus":"disabled","projects":{"/project":{"hasTrustDialogAccepted":true}}}' > /home/claude/.claude.json

ENTRYPOINT ["/home/claude/.local/bin/claude"]
