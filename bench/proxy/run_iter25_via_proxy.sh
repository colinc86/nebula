#!/usr/bin/env bash
# Run the iter-23/25 real-world benchmark against the FT v3 mini via
# LiteLLM proxy instead of the native Anthropic API.
#
# Prereqs:
#   - LiteLLM proxy running on :4000 (start with bench/proxy/start_proxy.sh).
#   - /tmp/gdsp clean working copy.
#   - Plugin v0.2 built (compiler at release target).
#
# This wires the proxy env vars then invokes iter23_piecewise.py unchanged —
# the harness spawns `claude --model sonnet`, which now routes through the
# proxy to ft:...nebulahost-pilot-v3:DWlP12V3.

set -euo pipefail
cd "$(dirname "$0")/../.."

# Verify proxy is live.
if ! curl -s -o /dev/null -w "%{http_code}" http://localhost:4000/health/liveliness | grep -q 200; then
    echo "ERROR: proxy not responding on :4000. Run bench/proxy/start_proxy.sh first." >&2
    exit 1
fi

export ANTHROPIC_BASE_URL="http://localhost:4000"
export ANTHROPIC_AUTH_TOKEN="sk-nebula-proxy-local"

# Pass through to iter23_piecewise.py. The harness takes task + mode args;
# forward all script args.
.venv/bin/python3 bench/mcp_manual_test/iter23_piecewise.py "$@"
