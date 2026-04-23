#!/usr/bin/env bash
# Start the LiteLLM proxy on port 4000, loading OPENAI_API_KEY from .env.
#
# Listens on http://localhost:4000 with Anthropic-compatible routes. Use:
#   export ANTHROPIC_BASE_URL=http://localhost:4000
#   export ANTHROPIC_AUTH_TOKEN=sk-nebula-proxy-local
# then invoke Claude Code normally. The `sonnet` / `claude-sonnet-4-5`
# model names transparently route to the FT mini.
#
# Logs go to bench/proxy/proxy.log. Stop with `pkill -f "litellm --config"`.

set -euo pipefail
cd "$(dirname "$0")/../.."

# Load OPENAI_API_KEY from .env. Strip surrounding double/single quotes if
# the .env value is wrapped (otherwise litellm forwards literal quotes to
# OpenAI and every request 401s).
if [[ -f .env ]]; then
    line=$(grep -E '^OPENAI_API_KEY=' .env | head -1)
    value=${line#OPENAI_API_KEY=}
    value=${value%\"}
    value=${value#\"}
    value=${value%\'}
    value=${value#\'}
    export OPENAI_API_KEY="$value"
fi
if [[ -z "${OPENAI_API_KEY:-}" ]]; then
    echo "ERROR: OPENAI_API_KEY not set (checked env and .env)." >&2
    exit 1
fi

# Kill any stale proxy on :4000 so re-runs don't stack.
pkill -f "litellm --config" 2>/dev/null || true
sleep 1

LOG=bench/proxy/proxy.log
echo "Starting LiteLLM proxy on :4000 → log $LOG"
.venv/bin/litellm --config bench/proxy/litellm_config.yaml --port 4000 > "$LOG" 2>&1 &
PROXY_PID=$!
echo "proxy pid: $PROXY_PID"

# Wait up to 15s for the proxy to answer /health.
for i in $(seq 1 15); do
    if curl -s -o /dev/null -w "%{http_code}" http://localhost:4000/health/liveliness | grep -q 200; then
        echo "proxy is live"
        exit 0
    fi
    sleep 1
done
echo "ERROR: proxy did not come up in 15s. See $LOG." >&2
tail -20 "$LOG"
exit 1
