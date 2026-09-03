# Shared plumbing for direct `avila-core run` invocations in the adversarial
# arm. Source this, then run e.g.:
#   "${CORE[@]}" run examples/cases/case-002-coupled-shield --json "${ROOTS[@]}" "${CAPS[@]}" --env OPENMC_CROSS_SECTIONS="$XS" --input candidate=PATH [--plan]
CORE=(/workspace/avila-core/target/debug/avila-core)
ROOTS=(
  --source-root case=examples/cases/case-002-coupled-shield
  --source-root shielding=examples/capabilities/shielding
  --source-root coupled=examples/capabilities/shield-coupled
  --source-root agents=examples/agents
  --source-root nuclear-data=/home/connoravila/nuclear-data/endfb-vii.1-hdf5
  --source-root actinv-release=/home/connoravila/Documents/actinv/target/release
  --source-root actinv-data=/home/connoravila/Documents/Avila-Labs/project-aftermatter/.data/actinv/v1.0.0
)
CAPS=(
  --capability python3=/usr/bin/python3
  --capability openmc-python=/home/connoravila/.venvs/w003env/bin/python3.12
)
XS=/home/connoravila/nuclear-data/endfb-vii.1-hdf5/cross_sections.xml
