# Shield activation (`activate.py`)

`activate.py` is interface I4's producer for the coupled slab-shield search:
given the per-layer neutron spectra a transport run computed (interface I3,
`avila.shielding/layer-spectra/v1`), it irradiates each layer's own material
with [ACTINV](https://github.com/AvilaLabs/ACTINV) under one declared
schedule and reports specific activity, decay heat, and — only when ACTINV's
own photon-response machinery can produce one — a contact-gamma dose-rate
proxy, at the end of the declared cooling period
(`avila.shielding/activation-result/v1`). It is `python3` only (system
interpreter, standard library, no network access) and is driven by the Rust
adapter `avila-labs.shielding/activation@1`
(`crates/avila-core-runner/src/execute/activation.rs`).

This directory does not touch `examples/capabilities/shielding/` (CASE-001's
frozen screen/transport scripts and material table) or CASE-000/CASE-001
themselves; `materials.json` is read from there, unmodified, by both.

## What it does, per layer

1. Look up the layer's material in the shared table
   (`examples/capabilities/shielding/materials.json`).
2. Convert that material's composition to ACTINV's `wt_percent` basis. An
   `"ao"` (atom-fraction) entry is converted using standard atomic weights
   embedded in the script (`ATOMIC_WEIGHT_G_PER_MOL`); a `"wo"` (mass-
   fraction) entry only needs normalizing to 100 — every material in the
   table goes through one code path either way.
3. Compute the layer's mass as `density_g_cm3 * thickness_cm * 10000 cm^2`,
   the same 1 m^2 nominal area convention I3's own `volume_cm3` field uses
   (`thickness_cm * 10000 == volume_cm3` in every I3 layer).
4. Build an `actinv-spec-1` problem: the layer's own I3 spectrum as a
   `custom`-structure spectrum with `descending: false` (see "the spectrum
   order" below), the converted material, and a two-step schedule —
   irradiate at `flux_scale` for `irradiation_s`, then cool for `cooling_s`.
5. Run `actinv validate` then `actinv run` as subprocesses with a cleared
   environment (only `ACTINV_CACHE_DIR`, resolved to an absolute path, which
   ACTINV requires).
6. Read the result's last step (end of cooling): sum `activity_Bq_per_g`
   for specific activity, `heat_W_per_g.total` (x1000) for decay heat in
   W/kg, and `photon_source.contact_gamma_air_dose_proxy_Gy_h` (x1e6, Gy
   treated as numerically equal to Sv for photons) for the contact-dose
   proxy when ACTINV produced one.
7. Write the I4 document: sorted keys, 2-space indent, every claim value an
   exact decimal string rounded to 8 significant figures (mirroring
   `examples/capabilities/shielding/transport.py`'s `stable()`/`canonical()`
   convention).

## What ACTINV offers here, and what was chosen

ACTINV's result reports far more than this script extracts: full nuclide
inventories, alpha/beta/gamma decay-heat components, ranked production
pathways, an MF=33 uncertainty band, user-selected clearance/waste/ingestion/
dose responses from a hash-pinned `actinv-radiological-table-1` file, and a
detailed ledger. I4 asks for three things — specific activity, decay heat,
and (if available) a contact dose-rate proxy — so this script requests only
`activity`, `heat`, `photons`, `ledger`, and `certificate` in
`options.outputs`, and reports:

- **Specific activity**: `sum(activity_Bq_per_g.values())` at the end of
  cooling. Always available.
- **Decay heat**: `heat_W_per_g.total` at the end of cooling, in W/kg.
  Always available.
- **Contact dose-rate proxy**: ACTINV's `contact_gamma_air_dose_proxy_Gy_h`
  (docs/METHOD.md's P7 semi-infinite-slab screening expression) requires an
  `actinv-photon-response-1` table with attenuation curves for the
  material's own elements. This adapter's input slots
  (`script, spectra, materials, schedule, actinv, activation-library,
  activation-index, decay-primary, decay-fallback`) do not include one — the
  coupled-shield spec's declared local ACTINV data release has no photon-
  response file — so every run today omits `contact_dose_rate` from each
  layer and `max_contact_dose_rate` from `totals`, and records why in each
  layer's `omissions`. The Rust adapter still declares the
  `contact-dose-rate` output slot and extracts it whenever a future run's I4
  document does carry `totals.max_contact_dose_rate`, with no adapter
  change needed.
- **MF=33 uncertainty, radiological responses, pathways, full inventory**:
  not requested. I4's own basis is nominal ("ACTINV reports no per-value
  bound for this use"), so an uncertainty band would not currently change
  any claim's model; a radiological-table selection is a policy choice this
  slice does not make.

## The spectrum order

ACTINV's `spectrum.descending` flag says whether the given `flux_per_group`
array is listed highest-energy-first (`true`, "as FISPACT fluxes files
are") or already ascending (`false`, the default). I3's `boundaries_eV` is
exactly ACTINV's own published FISPACT-709 group file
(`crates/actinv-data/data/fispact_709_groups.json`, "descending as
published") reversed to ascending — confirmed empirically, not just by
name: a `structure: "custom"` spec built from the reversed boundaries and
run through the real activation library succeeds, and perturbing one
interior boundary by 1% makes ACTINV refuse it ("custom spectrum boundaries
do not match the activation library boundaries"). I3's own layer flux
arrays are therefore already in ACTINV's ascending convention, group for
group, with no reversal needed.

This script goes one step further than trusting that alignment by name: it
builds every ACTINV spec with `spectrum.structure: "custom"` and I3's own
`boundaries_eV`, rather than the built-in `"fispact-709"` structure name.
ACTINV then independently re-verifies the boundaries against its own
activation library before every run — the same check that catches a
perturbed boundary above — instead of this script silently trusting that
I3's `group_structure: "fispact-709"` label is correct. This is a
deliberate deviation from a literal "pass fispact-709 by name" reading: it
costs nothing (the boundaries are already in the I3 file) and turns a
possible silent misalignment between slice A's I1 file and ACTINV's own
group data into a fail-closed error.

## The fixture is synthetic

`fixtures/layer-spectra.synthetic.json` is **not** a transport result. It is
a two-layer candidate (40 cm polyethylene, then 10 cm iron, both over the
shared 10000 cm^2 / 1 m^2 area convention) built directly by this slice for
development, before slice A's transport step existed:

- The flux **shape** (709 group values) is the FNS SS316 1996 experiment
  spectrum shape already in this repository
  (`project-aftermatter/cases/r0/input/fns-spectrum.json`, itself sourced
  from ACTINV's own `examples/fns_fe_5min.json`), reversed from ACTINV's
  descending convention to I3's ascending one.
- Layer 0 (polyethylene) is that shape scaled to a round 1.0e7 n/cm^2/s
  illustrative total.
- Layer 1 (iron) is layer 0's spectrum attenuated by a single uniform
  factor, `exp(-0.110 /cm * 40 cm)`, using polyethylene's own removal cross
  section from `examples/capabilities/shielding/materials.json`. This is a
  one-group removal proxy for "plausible," not a transport result — it does
  not reproduce any energy-dependent softening a real slab would produce.
- `boundaries_eV` are ACTINV's real, verified FISPACT-709 boundaries
  (see above), so the fixture is bit-exact where it claims to be exact.

`fixtures/schedule.json` (schema `avila.shielding/irradiation-schedule/v1`)
is 30 days irradiation, 1 day cooling, `flux_scale: "1"`.

If slice A's committed reference sample
(`samples/reference/layer-spectra.json` in the transport worktree) had
appeared before this slice was finished, it would have been run too; it had
not (see the report for what was checked instead).

## Running it

```bash
ulimit -v 12000000
/usr/bin/python3 examples/capabilities/shield-coupled/activate.py \
  --spectra examples/capabilities/shield-coupled/fixtures/layer-spectra.synthetic.json \
  --materials examples/capabilities/shielding/materials.json \
  --schedule examples/capabilities/shield-coupled/fixtures/schedule.json \
  --actinv /path/to/actinv \
  --activation-library /path/to/tendl-2025-neutron-709g.npz \
  --activation-index /path/to/tendl-2025-neutron-709g_index.json \
  --decay-primary /path/to/endf-b-viii-0_decay.dat \
  --decay-fallback /path/to/jeff-3-3_decay.dat \
  --output activation-result.json
```

`--activation-index` must sit next to `--activation-library` under the name
ACTINV itself derives (`<stem>.npz` -> `<stem>_index.json`,
`actinv_data::builder::index_path`'s rule) — the script checks this and
fails closed before running anything if it does not.

Tests (real ACTINV, real data, no mocks):

```bash
/usr/bin/python3 -m unittest examples/capabilities/shield-coupled/test_activate.py -v
```

## Limitations (see the I4 document's own `limitations` for the full list)

Nominal only (no per-value uncertainty bound); no transported shutdown dose
(this is a 0-D per-layer activation calculation, not photon transport
through the geometry); the contact-dose figure, when present, is ACTINV's
semi-infinite homogeneous-slab air-kerma screening proxy, not a transported
or finite-geometry dose rate; each layer is activated independently with no
inter-layer coupling; the bound activation/decay data pairing has its own
scope and known gaps (recorded once, at the document level, as a property of
the data release rather than of any one layer's composition — see
`activate.py`'s `layer_omissions()` docstring for why the two largest ledger
lists are deliberately not repeated per layer).
