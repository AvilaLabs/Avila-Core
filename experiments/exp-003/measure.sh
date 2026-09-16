#!/usr/bin/env bash
# EXP-003 harness: time the non-agent launch-to-report path on the frozen
# CASE-003 workload (workload.json). Records machine, build, workload
# identities, cache policy, repetitions, and available executables before
# any measurement, then times verify (run --plan), reuse (run), and fresh
# execution (run --no-reuse) separately. Phases whose pinned executables do
# not resolve are recorded UNAVAILABLE with the probe evidence; nothing is
# substituted for them. The agent and unattended-orchestration portions of
# EXP-003 are outside this harness entirely.
#
# Usage: experiments/exp-003/measure.sh [REPS]   (default: workload.json's 3)
set -euo pipefail
cd "$(dirname "$0")/../.."

CASE=examples/cases/case-003-thermal-spreader
THERMAL=examples/capabilities/thermal
TRUST=examples/keys/trust-root.json
KEY=examples/keys/runner.seed
CANDIDATE=$CASE/candidates/reference.json
REPS="${1:-3}"
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
OUT=experiments/exp-003/results/run-$STAMP
mkdir -p "$OUT"

sha() { sha256sum "$1" | cut -d' ' -f1; }

echo "== release build =="
cargo build --release -p avila-core-cli
CORE=target/release/avila-core

# -- manifest: everything recorded before the first measurement -----------
{
  echo "{"
  echo "  \"recorded_at\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\","
  echo "  \"machine\": {"
  echo "    \"kernel\": \"$(uname -srm)\","
  echo "    \"cpu\": \"$(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2- | sed 's/^ //' || echo unknown)\","
  echo "    \"host\": \"$(hostname)\""
  echo "  },"
  echo "  \"build\": {"
  echo "    \"commit\": \"$(git rev-parse HEAD)\","
  echo "    \"cli_sha256\": \"sha256:$(sha "$CORE")\","
  echo "    \"rustc\": \"$(rustc --version)\""
  echo "  },"
  echo "  \"workload_identities\": {"
  echo "    \"package_sha256\": \"sha256:$(sha "$CASE/package.json")\","
  echo "    \"contract_sha256\": \"sha256:$(sha "$CASE/contract.json")\","
  echo "    \"requirement_set_sha256\": \"sha256:$(sha "$CASE/requirement-set.json")\","
  echo "    \"candidate_sha256\": \"sha256:$(sha "$CANDIDATE")\""
  echo "  },"
  echo "  \"cache_policy\": \"committed-receipt reuse unless stated; no hash cache; scratch log $OUT/campaign.jsonl\","
  echo "  \"repetitions\": $REPS,"
  echo "  \"executables\": ["
  FIRST=1
  for cap in python3 thermal-python; do
    if command -v python3 >/dev/null; then
      PY=$(command -v python3)
      PYSHA=$(sha "$PY")
      PYVER=$("$PY" --version 2>&1)
    else
      PY=""; PYSHA=""; PYVER="absent"
    fi
    MODULE=absent
    if [ "$cap" = thermal-python ] && [ -n "$PY" ]; then
      if "$PY" -c "import skfem" 2>/dev/null; then MODULE=present; fi
    fi
    [ "$FIRST" = 1 ] && FIRST=0 || echo ","
    echo "    {"
    echo "      \"capability_id\": \"$cap\","
    echo "      \"resolved_path\": \"$PY\","
    echo "      \"resolved_sha256\": \"sha256:$PYSHA\","
    echo "      \"resolved_version\": \"$PYVER\","
    echo "      \"module_probe\": \"$MODULE\","
    echo "      \"pinned_sha256\": \"sha256:b8d8288faefdd300201f43fcf00f6f539a27218eeed3a3dff5ab10b9c4c99700\","
    echo "      \"pinned_match\": $([ "$PYSHA" = "b8d8288faefdd300201f43fcf00f6f539a27218eeed3a3dff5ab10b9c4c99700" ] && echo true || echo false)"
    echo -n "    }"
  done
  echo ""
  echo "  ],"
} > "$OUT/manifest-head.json"

run_phase() {
  # $1 phase name, rest: extra args
  local phase=$1; shift
  local i t0 t1
  for i in $(seq 1 "$REPS"); do
    t0=$(date +%s%N)
    "$CORE" run "$CASE" \
      --source-root "case=$CASE" \
      --source-root "thermal=$THERMAL" \
      --input "candidate=$CANDIDATE" \
      --log "$OUT/campaign.jsonl" \
      --trust-root "$TRUST" --runner-key "$KEY" "$@" \
      > "$OUT/$phase-$i.stdout" 2> "$OUT/$phase-$i.stderr" || true
    t1=$(date +%s%N)
    echo "    { \"repetition\": $i, \"wall_ms\": $(( (t1 - t0) / 1000000 )) },"
  done
}

echo "== verify: run --plan ($REPS reps) =="
{
  echo "  \"phases\": {"
  echo "    \"verify\": { \"command\": \"run --plan\", \"runs\": ["
  run_phase verify --plan | sed '$ s/,$//'
  echo "    ] },"
} >> "$OUT/manifest-head.json"

echo "== reuse: run ($REPS reps) =="
{
  echo "    \"reuse\": { \"command\": \"run\", \"runs\": ["
  run_phase reuse | sed '$ s/,$//'
  echo "    ] },"
} >> "$OUT/manifest-head.json"

# Fresh execution needs the pinned executables; record it unavailable with
# the probe evidence rather than substituting or faking a run.
FRESH=unavailable
REASON="pinned capability executables do not resolve on this machine"
if [ "$(command -v python3 >/dev/null && sha256sum "$(command -v python3)" | cut -d' ' -f1)" = "b8d8288faefdd300201f43fcf00f6f539a27218eeed3a3dff5ab10b9c4c99700" ] \
   && python3 -c "import skfem" 2>/dev/null; then
  FRESH=available
fi
{
  if [ "$FRESH" = available ]; then
    echo "    \"fresh\": { \"command\": \"run --no-reuse\", \"runs\": ["
    run_phase fresh --no-reuse | sed '$ s/,$//'
    echo "    ] },"
  else
    echo "    \"fresh\": { \"command\": \"run --no-reuse\", \"status\": \"UNAVAILABLE\", \"reason\": \"$REASON; see executables\" },"
  fi
  echo "    \"agent_orchestration\": { \"status\": \"UNAVAILABLE\", \"reason\": \"no agent harness in this measurement; EXP-003's agent and unattended portions are not claimed\" },"
  echo "    \"manual_setup\": { \"status\": \"performed once, not timed\", \"steps\": \"open case, point source roots at case/ and examples/capabilities/thermal, select trust root, runner key, and candidate file\" }"
  echo "  },"
  echo "  \"scratch_log\": \"$OUT/campaign.jsonl\""
  echo "}"
} >> "$OUT/manifest-head.json"

mv "$OUT/manifest-head.json" "$OUT/result.json"
echo "== wrote $OUT/result.json =="
