FROM debian:trixie-slim

ARG IMAGE_HASH
LABEL contenant.hash=${IMAGE_HASH}

RUN apt-get update && apt-get install -y \
    curl \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash claude

USER claude
WORKDIR /home/claude
ENV PATH="/home/claude/.local/bin:$PATH"

# Install Claude Code via native installer
RUN curl -fsSL https://claude.ai/install.sh | bash

# Pre-configure Claude to skip onboarding and trust /workspace
COPY claude.json /home/claude/.claude.json

ENTRYPOINT ["claude"]
