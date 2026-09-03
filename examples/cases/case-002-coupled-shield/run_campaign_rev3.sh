#!/usr/bin/env bash
# Revision 3's scripted arms (amendment A5): sweep on a 5 cm polyethylene-lead
# grid, the confined recovery run, and the seeded surrogate designer. The
# language-model arm is driven by a connected agent through
# shield_llm_tools.py and is launched separately; random search reuses
# revision 2's arm.
set -u
OUT="${1:?OUT_DIR}"
CORE="${CORE:-target/debug/avila-core}"
CASE="${CASE:-examples/cases/case-002-coupled-shield}"
NUCLEAR_DATA="${NUCLEAR_DATA:-$HOME/nuclear-data/endfb-vii.1-hdf5}"
OPENMC_PYTHON="${OPENMC_PYTHON:-$HOME/.venvs/w003env/bin/python3.12}"
ACTINV_RELEASE="${ACTINV_RELEASE:-$HOME/Documents/actinv/target/release}"
ACTINV_DATA="${ACTINV_DATA:-$HOME/Documents/Avila-Labs/project-aftermatter/.data/actinv/v1.0.0}"
PY=/usr/bin/python3
export OMP_NUM_THREADS="${OMP_NUM_THREADS:-8}"
if [ -z "${AVILA_CAMPAIGN_INHIBITED:-}" ] && command -v systemd-inhibit > /dev/null; then
  export AVILA_CAMPAIGN_INHIBITED=1
  exec systemd-inhibit --what=sleep:idle --who="avila-core campaign" --why="CASE-002 revision 3 campaign" "$0" "$@"
fi
mkdir -p "$OUT"; STATUS="$OUT/STATUS"
stamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }
say() { echo "$(stamp) $*" | tee -a "$STATUS"; }
COMMON=(--case "$CASE" --nuclear-data "$NUCLEAR_DATA" --cross-sections "$NUCLEAR_DATA/cross_sections.xml"
        --openmc-python "$OPENMC_PYTHON" --core "$CORE"
        --source-root "coupled=examples/capabilities/shield-coupled"
        --source-root "actinv-release=$ACTINV_RELEASE" --source-root "actinv-data=$ACTINV_DATA")
PRIOR=(--prior-log "$CASE/campaign-rev1/learning/campaign-log.jsonl" --prior-log "$CASE/campaign-rev1/random/campaign-log.jsonl"
       --prior-log "$CASE/campaign-rev1/sweep/campaign-log.jsonl" --prior-log "$CASE/campaign-rev1/recovery/campaign-log.jsonl"
       --prior-log "$CASE/campaign-rev1/baselines/campaign-log.jsonl"
       --prior-log "$CASE/campaign-rev2/learning/campaign-log.jsonl" --prior-log "$CASE/campaign-rev2/random/campaign-log.jsonl"
       --prior-log "$CASE/campaign-rev2/sweep/campaign-log.jsonl" --prior-log "$CASE/campaign-rev2/recovery/campaign-log.jsonl"
       --prior-log "$CASE/campaign-rev2/baselines/campaign-log.jsonl" --prior-log "$CASE/campaign-rev2/posthoc/campaign-log.jsonl")
run_stage() { local name="$1"; shift; say "START $name"; if "$@" > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"; then say "DONE $name"; else say "FAILED $name exit=$? (see $OUT/$name.stderr)"; fi; }
say "CAMPAIGN START case=$CASE revision 3 scripted arms"
run_stage sweep $PY examples/agents/control_sweep.py sweep "${COMMON[@]}" \
  --materials polyethylene lead --grid-cm 5 --max-layers 2 --min-total-cm 100 --first-material polyethylene --out "$OUT/sweep"
run_stage recovery $PY examples/agents/shield_search2.py "${COMMON[@]}" \
  --materials polyethylene lead --grid-cm 5 --max-layers 2 --screen-budget 400 --transport-budget 15 \
  --batch-size 10 --patience 5 --patience-transports 6 --finalists-per-round 3 --seed 1 --out "$OUT/recovery"
run_stage surrogate $PY examples/agents/shield_search2.py "${COMMON[@]}" "${PRIOR[@]}" \
  --grid-cm 5 --max-layers 3 --screen-budget 2000 --transport-budget 40 --batch-size 20 \
  --patience 5 --patience-transports 10 --finalists-per-round 3 --seed 1 --out "$OUT/surrogate"
say "CAMPAIGN END"
