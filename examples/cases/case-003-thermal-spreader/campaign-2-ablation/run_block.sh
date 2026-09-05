#!/bin/bash
# EXP-002 scored block, frozen per experiments/EXP-002.protocol_hash.txt (commit b872f15).
cd /home/connoravila/Documents/Avila-Labs/project-north-star || exit 1
OUT=workspaces/exp-002/block-1
mkdir -p "$OUT"
echo "START $(date -u +%FT%TZ)" >> workspaces/exp-002/STATUS
systemd-inhibit --what=sleep:idle --why="EXP-002 scored block" \
  python3 examples/agents/ablation/harness.py run_block \
    --trials 5 --seed 20260904 \
    --out "$OUT" \
    --screen-budget 40 --eval-budget 12 --n-eval 12 \
    --timeout-s 1800 --max-retries 1 --max-budget-usd 5.0 \
    --core workspaces/exp-002/avila-core-frozen \
  > workspaces/exp-002/block-1.log 2>&1
rc=$?
echo "BLOCK END rc=$rc $(date -u +%FT%TZ)" >> workspaces/exp-002/STATUS
