#!/usr/bin/env bash
# Runs the CASE-002 campaign exactly as PROTOCOL.md pre-declares it, one arm
# after another, writing a status line per stage to $OUT/STATUS so an
# unattended run can be followed. Every Core run is logged by the arm's own
# --log file; nothing here constructs a verdict.
#
# Usage: run_campaign.sh OUT_DIR
# Environment (defaults are this machine's paths):
#   CORE, CASE, NUCLEAR_DATA, OPENMC_PYTHON, ACTINV_RELEASE, ACTINV_DATA
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
mkdir -p "$OUT"
STATUS="$OUT/STATUS"
stamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }
say() { echo "$(stamp) $*" | tee -a "$STATUS"; }

COMMON=(--case "$CASE" --nuclear-data "$NUCLEAR_DATA"
        --cross-sections "$NUCLEAR_DATA/cross_sections.xml"
        --openmc-python "$OPENMC_PYTHON" --core "$CORE"
        --source-root "coupled=examples/capabilities/shield-coupled"
        --source-root "actinv-release=$ACTINV_RELEASE"
        --source-root "actinv-data=$ACTINV_DATA")

run_stage() {
  local name="$1"; shift
  say "START $name"
  if "$@" > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"; then
    say "DONE $name"
  else
    say "FAILED $name exit=$? (see $OUT/$name.stderr)"
  fi
}

say "CAMPAIGN START case=$CASE"

# Arm 1: practice baselines, every step (screen, transport, activation).
run_stage baselines $PY examples/agents/practice_baseline.py "${COMMON[@]}" \
  --run --out "$OUT/baselines"

# Arm 4 (control): exhaustive sweep of the declared sub-grid: one or two
# layers of polyethylene and lead on a 10 cm grid within the limits.
run_stage sweep $PY examples/agents/control_sweep.py sweep "${COMMON[@]}" \
  --materials polyethylene lead --grid-cm 10 --max-layers 2 --out "$OUT/sweep"

# Recovery check: the learning designer restricted to the sweep's space.
run_stage recovery $PY examples/agents/shield_search2.py "${COMMON[@]}" \
  --materials polyethylene lead --grid-cm 10 --max-layers 2 \
  --screen-budget 400 --transport-budget 15 --batch-size 10 --patience 5 \
  --seed 1 --out "$OUT/recovery"

# Arm 3: the learning designer over the full space.
run_stage learning $PY examples/agents/shield_search2.py "${COMMON[@]}" \
  --grid-cm 5 --max-layers 3 --screen-budget 2000 --transport-budget 40 \
  --batch-size 20 --patience 5 --seed 1 --out "$OUT/learning"

# Arm 2: random search over the same space with the same budgets.
run_stage random $PY examples/agents/shield_search2.py "${COMMON[@]}" --random \
  --grid-cm 5 --max-layers 3 --screen-budget 2000 --transport-budget 40 \
  --batch-size 20 --patience 5 --seed 1 --out "$OUT/random"

say "CAMPAIGN END"
