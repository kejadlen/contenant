FROM debian:trixie-slim

RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    ca-certificates \
    nftables \
    iproute2 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash claude

# Install Claude Code as claude user
USER claude
WORKDIR /home/claude
ENV PATH="/home/claude/.local/bin:$PATH"

# Create .ssh directory for known_hosts mount
RUN mkdir -p /home/claude/.ssh && chmod 700 /home/claude/.ssh

# Install Claude Code via native installer
ARG CLAUDE_VERSION=
RUN curl -fsSL https://claude.ai/install.sh | bash${CLAUDE_VERSION:+ -s -- $CLAUDE_VERSION}

# Pre-configure Claude to skip onboarding and trust /workspace
COPY claude.json /home/claude/.claude.json

# Entrypoint runs as root to configure firewall, then drops to claude
USER root
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
