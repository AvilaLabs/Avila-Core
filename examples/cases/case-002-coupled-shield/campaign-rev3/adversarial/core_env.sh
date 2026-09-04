# Shared plumbing for direct `avila-core run` invocations in the adversarial
# arm. Source this, then run e.g.:
#   "${CORE[@]}" run examples/cases/case-002-coupled-shield --json "${ROOTS[@]}" "${CAPS[@]}" --env OPENMC_CROSS_SECTIONS="$XS" --input candidate=PATH [--plan]
: "${OPENMC_DATA_ROOT:?Set OPENMC_DATA_ROOT to the OpenMC data-library directory}"
: "${ACTINV_RELEASE_ROOT:?Set ACTINV_RELEASE_ROOT to the directory containing actinv}"
: "${ACTINV_DATA_ROOT:?Set ACTINV_DATA_ROOT to the ACTINV data-release directory}"
: "${OPENMC_PYTHON:?Set OPENMC_PYTHON to the OpenMC-enabled interpreter}"
: "${OPENMC_CROSS_SECTIONS:=$OPENMC_DATA_ROOT/cross_sections.xml}"

CORE=("${AVILA_CORE_BIN:-target/debug/avila-core}")
ROOTS=(
  --source-root case=examples/cases/case-002-coupled-shield
  --source-root shielding=examples/capabilities/shielding
  --source-root coupled=examples/capabilities/shield-coupled
  --source-root agents=examples/agents
  --source-root "nuclear-data=$OPENMC_DATA_ROOT"
  --source-root "actinv-release=$ACTINV_RELEASE_ROOT"
  --source-root "actinv-data=$ACTINV_DATA_ROOT"
)
CAPS=(
  --capability python3=/usr/bin/python3
  --capability "openmc-python=$OPENMC_PYTHON"
)
XS="$OPENMC_CROSS_SECTIONS"
