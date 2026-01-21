FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    curl \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 20 LTS
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Claude Code
RUN npm install -g @anthropic-ai/claude-code

# Create non-root user
RUN useradd -m -s /bin/bash claude

# Skip onboarding and trust /project
RUN echo '{"hasCompletedOnboarding":true,"projects":{"/project":{"hasTrustDialogAccepted":true}}}' > /home/claude/.claude.json \
    && chown claude:claude /home/claude/.claude.json

USER claude
WORKDIR /home/claude

ENTRYPOINT ["claude"]
