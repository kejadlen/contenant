#!/bin/bash
set -euo pipefail
IFS=$'\n\t'

# Build the set of allowed CIDRs from the file mounted by contenant
ALLOWED_ELEMENTS=""
while IFS= read -r cidr; do
    if [ -n "$cidr" ]; then
        [ -n "$ALLOWED_ELEMENTS" ] && ALLOWED_ELEMENTS+=", "
        ALLOWED_ELEMENTS+="$cidr"
    fi
done < /etc/contenant/allowed-ips

# Discover host network (for Docker communication and bridge server)
HOST_IP=$(ip route | grep default | cut -d" " -f3)
HOST_NETWORK=$(echo "$HOST_IP" | sed "s/\.[0-9]*$/.0\/24/")

# Load the entire ruleset atomically
nft -f - <<EOF
table inet contenant {
    set allowed-ips {
        type ipv4_addr
        flags interval
        $([ -n "$ALLOWED_ELEMENTS" ] && echo "elements = { $ALLOWED_ELEMENTS }")
    }

    chain output {
        type filter hook output priority 0; policy drop;

        # Allow loopback
        oifname "lo" accept

        # Allow DNS (required for Docker's embedded DNS at 127.0.0.11)
        udp dport 53 accept

        # Allow SSH (for git operations)
        tcp dport 22 accept

        # Allow host network (Docker communication + bridge server)
        ip daddr $HOST_NETWORK accept

        # Allow established/related connections
        ct state established,related accept

        # Allow traffic to allowlisted IPs
        ip daddr @allowed-ips accept

        # Reject everything else with immediate feedback
        reject with icmpx admin-prohibited
    }

    chain input {
        type filter hook input priority 0; policy drop;

        # Allow loopback
        iifname "lo" accept

        # Allow established/related connections
        ct state established,related accept

        # Allow host network
        ip saddr $HOST_NETWORK accept
    }
}
EOF

# Drop privileges and run Claude Code
exec su -s /bin/bash claude -c "claude $*"
