#!/usr/bin/env bash
# Run the curated GAIA level-1 subset under two configs (builtins, delegate).
#
# Inputs:
#   - gaia_l1.jsonl         dataset from `datasets.load_dataset('gaia-benchmark/GAIA', ...)`
#   - subset.txt            one GAIA task id per line
# Outputs:
#   - runs/gaia_<id>_<cfg>/result.json
#   - summary.csv (appended)
set -euo pipefail
cd "$(dirname "$0")"

DATASET="gaia_l1.jsonl"
SUBSET="subset.txt"
if [[ ! -f "$DATASET" ]]; then
  echo "error: $DATASET not found. Run:" >&2
  echo "  python3 -c \"from datasets import load_dataset; load_dataset('gaia-benchmark/GAIA', '2023_level1', split='validation').to_json('$DATASET')\"" >&2
  exit 1
fi
if [[ ! -f "$SUBSET" ]]; then
  echo "error: $SUBSET not found (one task id per line)" >&2
  exit 1
fi

SUMMARY="summary.csv"
if [[ ! -f "$SUMMARY" ]]; then
  echo "task_id,config,total_cost_usd,input_tokens,cache_read_tokens,output_tokens,num_turns,duration_ms,predicted,expected,correct" > "$SUMMARY"
fi

run_one_task() {
  local task_id="$1" config="$2"
  local run_dir="runs/gaia_${task_id}_${config}"
  if [[ -f "$run_dir/result.json" && -s "$run_dir/result.json" ]]; then
    echo "[skip] $task_id $config"
    return
  fi
  rm -rf "$run_dir"
  mkdir -p "$run_dir"

  # Stage task: write question.txt, copy any attached files
  python3 stage_task.py "$task_id" "$run_dir" "$DATASET"

  local mcp_flag
  case "$config" in
    builtins) mcp_flag="--mcp-config ../../../mcp_manual_test/empty_mcp.json --strict-mcp-config" ;;
    delegate) mcp_flag="--mcp-config ../../../mcp_manual_test/delegate_stock_mcp.json --strict-mcp-config" ;;
  esac

  echo "[run ] $task_id $config"
  (
    cd "$run_dir"
    local PROMPT
    PROMPT="$(cat question.txt)"
    # shellcheck disable=SC2086
    claude --print \
      $mcp_flag \
      --permission-mode bypassPermissions --model sonnet \
      --output-format json --no-session-persistence --setting-sources user \
      --max-budget-usd 0.80 \
      "$PROMPT" > result.json 2> claude.err || true
  )
}

while read -r line; do
  # Strip comments and whitespace, keep only the first token (task_id)
  task_id=$(echo "$line" | sed 's/#.*//' | awk '{print $1}')
  [[ -z "$task_id" ]] && continue
  for cfg in builtins delegate; do
    run_one_task "$task_id" "$cfg"
  done
done < "$SUBSET"

echo
echo "All runs complete. Scoring..."
python3 score_gaia.py
