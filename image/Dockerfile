FROM debian:trixie-slim

RUN apt-get update && apt-get install -y \
    curl \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 24 LTS
RUN curl -fsSL https://deb.nodesource.com/setup_24.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install jj (Jujutsu VCS)
# TODO: multi-arch support (currently aarch64 only)
RUN curl -fsSL https://github.com/jj-vcs/jj/releases/download/v0.37.0/jj-v0.37.0-aarch64-unknown-linux-musl.tar.gz \
    | tar -xz -C /usr/local/bin ./jj

# Install Claude Code
RUN npm install -g @anthropic-ai/claude-code

# Create non-root user
RUN useradd -m -s /bin/bash claude

# Skip onboarding, trust /project, disable auto-update
RUN echo '{"hasCompletedOnboarding":true,"autoUpdaterStatus":"disabled","projects":{"/project":{"hasTrustDialogAccepted":true}}}' > /home/claude/.claude.json \
    && chown claude:claude /home/claude/.claude.json

USER claude
WORKDIR /home/claude

ENTRYPOINT ["claude"]
