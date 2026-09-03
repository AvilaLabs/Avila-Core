#!/usr/bin/env python3
"""A surrogate-assisted designer for the shielding configuration search.

`shield_search.py` (frozen) proposes uniformly random layered slabs. This
designer instead learns from Avila Core's own campaign log: it fits a small
ridge-regression surrogate per requirement over features of the candidate
geometry, uses the surrogate's predicted margins and its bootstrap
uncertainty to choose which candidates to screen and which screened
candidates are worth spending transport on, and stops when its budget is
exhausted or it stops improving. Every number it reports is read from Core's
`--json` run report or its `--log` campaign log; nothing here constructs or
alters a verdict, and no requirement id, limit, or unit is hard-coded — they
are discovered from a live report so the same script runs unchanged on
CASE-001 today and on the composed coupled case later (which adds photon
dose and activation requirements under different ids).

Two Core capabilities are used exactly as `shield_search.py` uses them:
python3-only for the cheap screen, both capabilities for transport. A
candidate is only ever "predicted feasible"; only Core's own verdicts, read
back from its report, ever say PASS, FAIL, INCONCLUSIVE, or NOT_EVALUATED.
"""

import argparse
import copy
import json
import math
from decimal import Decimal
from fractions import Fraction
from pathlib import Path

import numpy as np

import shield_common as core
from shield_common import STATUS_NOT_EVALUATED, STATUS_PASS

try:  # optional accelerant; a numpy-only closed form is always available.
    from sklearn.linear_model import Ridge as _SklearnRidge

    HAVE_SKLEARN = True
except Exception:  # pragma: no cover - exercised only where sklearn is absent
    HAVE_SKLEARN = False

SURROGATE_NOTICE = (
    "The surrogate only proposes candidates and predicts their margins; it "
    "never constructs, edits, or substitutes for a Core verdict. Every "
    "status, rule, limit, and margin in this file is copied from Avila "
    "Core's own --json report or --log campaign log."
)

# ----------------------------------------------------------------------
# Candidate geometry features
#
# "total thickness per material, layer count, first- and last-layer
# material one-hots, simple ordering features" (spec). The feature layout
# is a pure function of the material vocabulary, so it is identical for
# CASE-001 and the coupled case as long as both share a materials table.
# ----------------------------------------------------------------------

# "Heavy" is density above this threshold in the materials table;
# "moderator" is the rest. This is what the photon requirement responds
# to: a heavy layer behind the moderator attenuates the capture photons
# the moderator itself generates; one in front of the moderator does not
# (revision 2's learning designer never learned this and put its heavy
# layer in front every time it passed neutron).
HEAVY_DENSITY_THRESHOLD_G_CM3 = 2.0


def is_heavy(material, materials_table, threshold=HEAVY_DENSITY_THRESHOLD_G_CM3):
    return float(materials_table[material]["density_g_cm3"]) > threshold


class FeatureLayout:
    def __init__(self, material_names):
        self.material_names = list(material_names)
        self.index = {name: i for i, name in enumerate(self.material_names)}
        self.n_materials = len(self.material_names)

    def names(self):
        names = [f"thickness_cm:{m}" for m in self.material_names]
        names.append("layer_count")
        names += [f"first_layer_is:{m}" for m in self.material_names]
        names += [f"last_layer_is:{m}" for m in self.material_names]
        names += [
            "distinct_materials",
            "density_order_correlation",
            "front_loaded_mass_fraction",
            "adjacent_same_material_pairs",
            "moderator_cm_before_first_heavy",
            "heavy_cm_after_last_moderator",
            "heavy_cm_before_first_moderator",
            "moderator_total_cm",
            "heavy_total_cm",
            "last_layer_is_heavy",
        ]
        return names

    @property
    def n_features(self):
        return len(self.names())

    def vector(self, layers, materials_table):
        """One feature row for a candidate's `layers` list."""
        if not layers:
            raise ValueError("a candidate must have at least one layer")
        n = self.n_materials
        thickness_by_material = np.zeros(n)
        thicknesses = []
        densities = []
        for layer in layers:
            material = layer["material"]
            thickness = float(Fraction(str(layer["thickness_cm"])))
            i = self.index[material]
            thickness_by_material[i] += thickness
            thicknesses.append(thickness)
            densities.append(float(materials_table[material]["density_g_cm3"]))

        layer_count = float(len(layers))
        first_oh = np.zeros(n)
        first_oh[self.index[layers[0]["material"]]] = 1.0
        last_oh = np.zeros(n)
        last_oh[self.index[layers[-1]["material"]]] = 1.0

        distinct_materials = float(len({layer["material"] for layer in layers}))

        densities_arr = np.array(densities)
        if len(layers) >= 2 and densities_arr.std() > 0:
            positions = np.arange(len(layers), dtype=float)
            density_order_correlation = float(np.corrcoef(positions, densities_arr)[0, 1])
        else:
            density_order_correlation = 0.0

        total_thickness = sum(thicknesses)
        total_mass_proxy = sum(t * d for t, d in zip(thicknesses, densities)) or 1.0
        half = total_thickness / 2.0
        cumulative = 0.0
        front_mass = 0.0
        for t, d in zip(thicknesses, densities):
            covered = min(t, max(0.0, half - cumulative))
            front_mass += covered * d
            cumulative += t
        front_loaded_mass_fraction = front_mass / total_mass_proxy

        adjacent_same_material_pairs = float(
            sum(
                1
                for i in range(1, len(layers))
                if layers[i]["material"] == layers[i - 1]["material"]
            )
        )

        # Ordering-aware features. Each "before/after" pair is a leading or
        # trailing run: it stops at the first layer of the *other* class, so
        # it is naturally 0 when that other class never appears after (or
        # before) it, and it is naturally the full run when the anchor class
        # (heavy, for the "before/after moderator" pair; moderator, for the
        # "before first heavy" one) never appears in the stack at all -- a
        # vacuous but consistent reading, not a special case.
        heavy_flags = [d > HEAVY_DENSITY_THRESHOLD_G_CM3 for d in densities]
        moderator_cm_before_first_heavy = 0.0
        for t, heavy in zip(thicknesses, heavy_flags):
            if heavy:
                break
            moderator_cm_before_first_heavy += t
        heavy_cm_after_last_moderator = 0.0
        for t, heavy in zip(reversed(thicknesses), reversed(heavy_flags)):
            if not heavy:
                break
            heavy_cm_after_last_moderator += t
        heavy_cm_before_first_moderator = 0.0
        for t, heavy in zip(thicknesses, heavy_flags):
            if not heavy:
                break
            heavy_cm_before_first_moderator += t
        moderator_total_cm = sum(t for t, heavy in zip(thicknesses, heavy_flags) if not heavy)
        heavy_total_cm = sum(t for t, heavy in zip(thicknesses, heavy_flags) if heavy)
        last_layer_is_heavy = 1.0 if heavy_flags[-1] else 0.0

        return np.concatenate(
            [
                thickness_by_material,
                [layer_count],
                first_oh,
                last_oh,
                [
                    distinct_materials,
                    density_order_correlation,
                    front_loaded_mass_fraction,
                    adjacent_same_material_pairs,
                    moderator_cm_before_first_heavy,
                    heavy_cm_after_last_moderator,
                    heavy_cm_before_first_moderator,
                    moderator_total_cm,
                    heavy_total_cm,
                    last_layer_is_heavy,
                ],
            ]
        )

    def thickness_feature_index(self, material):
        return self.index[material]


# ----------------------------------------------------------------------
# Ridge regression, numpy closed-form with an optional scikit-learn fit,
# and a bootstrap ensemble for uncertainty.
# ----------------------------------------------------------------------


class RidgeModel:
    """log(nominal) ~ standardized features, ridge-penalized. Intercept is
    handled by centering rather than penalizing a bias column."""

    def __init__(self, alpha=1.0):
        self.alpha = float(alpha)
        self.x_mean = None
        self.x_scale = None
        self.y_mean = 0.0
        self.coef_ = None

    def fit(self, X, y):
        X = np.asarray(X, dtype=float)
        y = np.asarray(y, dtype=float)
        self.x_mean = X.mean(axis=0)
        scale = X.std(axis=0)
        scale[scale < 1e-9] = 1.0
        self.x_scale = scale
        Xc = (X - self.x_mean) / self.x_scale
        self.y_mean = float(y.mean())
        yc = y - self.y_mean
        if HAVE_SKLEARN:
            fitted = _SklearnRidge(alpha=self.alpha, fit_intercept=False)
            fitted.fit(Xc, yc)
            self.coef_ = np.asarray(fitted.coef_, dtype=float)
        else:
            n_features = Xc.shape[1]
            gram = Xc.T @ Xc + self.alpha * np.eye(n_features)
            self.coef_ = np.linalg.solve(gram, Xc.T @ yc)
        return self

    def predict(self, X):
        X = np.asarray(X, dtype=float)
        Xc = (X - self.x_mean) / self.x_scale
        return Xc @ self.coef_ + self.y_mean

    def raw_coefficients(self):
        """d(prediction)/d(raw feature), chain-ruled through standardization."""
        return self.coef_ / self.x_scale


class BootstrapRidge:
    """A ridge point estimate plus a bootstrap ensemble for its spread.
    `ready` is False until at least `min_samples` observations have been
    fit, so a requirement with little or no data is honestly "no model"
    rather than an overconfident line through two points.
    """

    def __init__(self, alpha=1.0, n_bootstrap=16, min_samples=5, seed=0):
        self.alpha = alpha
        self.n_bootstrap = n_bootstrap
        self.min_samples = min_samples
        self.rng = np.random.default_rng(seed)
        self.point_model = None
        self.ensemble = []
        self.n_samples = 0

    @property
    def ready(self):
        return self.point_model is not None

    def fit(self, X, y):
        X = np.asarray(X, dtype=float)
        y = np.asarray(y, dtype=float)
        self.n_samples = len(y)
        if self.n_samples < self.min_samples:
            self.point_model = None
            self.ensemble = []
            return self
        self.point_model = RidgeModel(self.alpha).fit(X, y)
        ensemble = []
        for _ in range(self.n_bootstrap):
            picks = self.rng.integers(0, self.n_samples, size=self.n_samples)
            ensemble.append(RidgeModel(self.alpha).fit(X[picks], y[picks]))
        self.ensemble = ensemble
        return self

    def predict(self, X):
        if not self.ready:
            raise RuntimeError("BootstrapRidge.predict() called before enough data was fit")
        X = np.asarray(X, dtype=float)
        mean = self.point_model.predict(X)
        if self.ensemble:
            spread = np.stack([m.predict(X) for m in self.ensemble])
            std = spread.std(axis=0)
        else:
            std = np.zeros_like(mean)
        return mean, std


# ----------------------------------------------------------------------
# The accumulated dataset: one row per candidate ever screened or
# transported, keyed by candidate_id, with every requirement's numbers
# copied verbatim from Core's margins.
# ----------------------------------------------------------------------


class CampaignDataset:
    def __init__(self, layout, materials_table):
        self.layout = layout
        self.materials_table = materials_table
        self.records = {}  # candidate_id -> record
        self.metric_sources = {}  # requirement_id -> {"step_id", "output_slot"}
        # requirement_id -> {"comparison", "limit", "unit"}, from the
        # *compiled contract*, present in a --json report the instant
        # compilation succeeds, whether or not anything has been evaluated
        # yet. This is what makes a requirement's limit and direction known
        # even before its first PASS/FAIL/INCONCLUSIVE observation -- a
        # NOT_EVALUATED verdict carries no limit at all (the kernel leaves
        # every number unset), so a requirement with zero observations of
        # its own (every requirement, before its first run) would otherwise
        # have no knowable limit or direction until this exists.
        self.requirement_specs = {}

    def add_report(self, candidate, report):
        candidate_id = candidate["candidate_id"]
        features = self.layout.vector(candidate["layers"], self.materials_table)
        record = self.records.setdefault(
            candidate_id,
            {"features": features, "layers": candidate["layers"], "requirements": {}},
        )
        compiled = ((report.get("compile") or {}).get("compiled")) or {}
        for spec in compiled.get("requirements", []):
            requirement_id = spec.get("requirement_id")
            if not requirement_id:
                continue
            metric = spec.get("metric") or {}
            self.metric_sources.setdefault(requirement_id, {
                "step_id": metric.get("step_id"), "output_slot": metric.get("output_slot"),
            })
            limit = spec.get("limit") or {}
            self.requirement_specs[requirement_id] = {
                "comparison": spec.get("comparison"),
                "limit": limit.get("value"),
                "unit": limit.get("unit"),
            }
        for requirement_id, source in core.metric_sources(report).items():
            self.metric_sources.setdefault(requirement_id, source)
        for entry in core.all_margins(report):
            requirement_id = entry["requirement_id"]
            data = {
                "status": entry["status"],
                "rule": entry.get("rule"),
                "unit": entry.get("unit"),
            }
            nominal_text = entry.get("nominal")
            limit_text = entry.get("limit")
            margin_text = entry.get("margin")
            if nominal_text is not None:
                nominal = float(Fraction(nominal_text))
                data["nominal"] = nominal
                if nominal > 0:
                    data["log_nominal"] = math.log(nominal)
            if limit_text is not None:
                data["limit"] = float(Fraction(limit_text))
            if margin_text is not None:
                data["margin"] = float(Fraction(margin_text))
            record["requirements"][requirement_id] = data
        return record

    def requirement_ids(self):
        ids = set()
        for record in self.records.values():
            ids.update(record["requirements"].keys())
        return ids

    def xy(self, requirement_id, key="log_nominal"):
        X, y, ids = [], [], []
        for candidate_id, record in self.records.items():
            data = record["requirements"].get(requirement_id)
            if data and key in data:
                X.append(record["features"])
                y.append(data[key])
                ids.append(candidate_id)
        if not X:
            return np.zeros((0, self.layout.n_features)), np.zeros(0), []
        return np.array(X), np.array(y), ids

    def known_layer_signatures(self):
        return {
            candidate_id: layer_signature(record["layers"])
            for candidate_id, record in self.records.items()
        }


layer_signature = core.layer_signature


def infer_direction(observations):
    """From (limit, nominal, margin) triples, whether this requirement's
    margin convention is `limit - nominal` (an upper-bound requirement,
    +1) or `nominal - limit` (a lower-bound requirement, -1). Inferred from
    Core's own reported numbers, never assumed from a comparison operator.
    """
    votes = {1: 0, -1: 0}
    for limit, nominal, margin_value in observations:
        if limit is None or nominal is None or margin_value is None:
            continue
        tolerance = max(1e-6, abs(margin_value) * 1e-6)
        if abs((limit - nominal) - margin_value) <= tolerance:
            votes[1] += 1
        elif abs((nominal - limit) - margin_value) <= tolerance:
            votes[-1] += 1
    if votes[1] == 0 and votes[-1] == 0:
        return None
    return 1 if votes[1] >= votes[-1] else -1


# Unit classification (mass-like, length-like, a dose rate) and the
# bounded/nominal rule-prefix helpers live in shield_common: generic and
# data-driven, so no requirement id is ever assumed here either.
unit_role = core.unit_role


class SurrogateBank:
    """One BootstrapRidge per requirement over its own data, plus an
    additive residual correction from any sibling requirement (same
    Core-reported metric output_slot, i.e. the same underlying quantity at
    a different fidelity) when the corrected requirement's own data is
    thin. This is how "the transport model corrects the screen model
    where transport data exist" is implemented without hard-coding which
    requirement is the cheap one and which is the qualified one: siblings
    are discovered from Core's own `metric.output_slot`, and whichever
    side has more data is used as the prior.
    """

    def __init__(self, dataset, alpha=2.0, n_bootstrap=16, min_samples=5, residual_min_samples=2, seed=0):
        self.dataset = dataset
        self.alpha = alpha
        self.n_bootstrap = n_bootstrap
        self.min_samples = min_samples
        self.residual_min_samples = residual_min_samples
        self.seed = seed
        self.own_models = {}
        self.residual_models = {}  # requirement_id -> (prior_requirement_id, BootstrapRidge)
        self.directions = {}
        self.limits = {}
        self.units = {}
        self.rules_seen = {}  # requirement_id -> set of rule prefixes observed

    def _seed_for(self, key):
        return (hash(key) & 0xFFFFFFFF) ^ self.seed

    def refit(self):
        requirement_ids = self.dataset.requirement_ids() | set(self.dataset.requirement_specs)
        for requirement_id in requirement_ids:
            X, y, _ids = self.dataset.xy(requirement_id, "log_nominal")
            observations = []
            limit_value = None
            unit_value = None
            rules = set()
            for record in self.dataset.records.values():
                data = record["requirements"].get(requirement_id)
                if not data:
                    continue
                if data.get("unit"):
                    unit_value = data["unit"]
                if "limit" in data:
                    limit_value = data["limit"]
                if data.get("rule"):
                    rules.add(data["rule"].split(".")[0] + ".")
                if all(k in data for k in ("limit", "nominal", "margin")):
                    observations.append((data["limit"], data["nominal"], data["margin"]))
            self.rules_seen[requirement_id] = rules
            # The compiled contract's own statement of a requirement's
            # limit, unit, and comparison is authoritative and available
            # the instant compilation succeeds -- in particular before this
            # requirement has ever been evaluated, when a NOT_EVALUATED
            # verdict alone would leave all three unknown. Observed margins
            # are the fallback for a report shape that has no compiled
            # section at all (a hand-built one, in a test, for instance).
            spec = self.dataset.requirement_specs.get(requirement_id)
            direction = None
            if spec and spec.get("limit") is not None and spec.get("unit") is not None:
                self.limits[requirement_id] = float(Fraction(spec["limit"]))
                self.units[requirement_id] = spec["unit"]
                comparison = spec.get("comparison")
                if comparison in ("less_than", "less_than_or_equal"):
                    direction = 1
                elif comparison in ("greater_than", "greater_than_or_equal"):
                    direction = -1
            else:
                if limit_value is not None:
                    self.limits[requirement_id] = limit_value
                if unit_value is not None:
                    self.units[requirement_id] = unit_value
            if direction is None:
                direction = infer_direction(observations)
            if direction is not None:
                self.directions[requirement_id] = direction
            model = self.own_models.setdefault(
                requirement_id,
                BootstrapRidge(
                    self.alpha, self.n_bootstrap, self.min_samples,
                    seed=self._seed_for(("own", requirement_id)),
                ),
            )
            model.fit(X, y)
        self._refit_residuals(requirement_ids)

    def _siblings(self, requirement_id):
        source = self.dataset.metric_sources.get(requirement_id)
        if not source or not source.get("output_slot"):
            return []
        slot = source["output_slot"]
        return [
            other
            for other, other_source in self.dataset.metric_sources.items()
            if other != requirement_id and other_source.get("output_slot") == slot
        ]

    def _refit_residuals(self, requirement_ids):
        """Register a sibling prior for every requirement whose own data is
        too thin, *including one with zero observations of its own* — that
        is exactly the state a requirement is in before its very first
        transport call, and it is precisely then that a prediction (even a
        crude, inflated-uncertainty one) is most useful for choosing what
        to spend that first call on. The residual correction on top of the
        prior only engages once `residual_min_samples` of the corrected
        requirement's own data exist; until then `predict_log_nominal`
        falls back to the sibling's prediction alone.
        """
        self.residual_models = {}
        for requirement_id in requirement_ids:
            best = None
            for sibling in self._siblings(requirement_id):
                sibling_model = self.own_models.get(sibling)
                if not sibling_model or not sibling_model.ready:
                    continue
                X, y, _ids = self.dataset.xy(requirement_id, "log_nominal")
                residual_model = None
                if len(y) >= self.residual_min_samples:
                    sibling_mean, _sibling_std = sibling_model.predict(X)
                    residual_y = y - sibling_mean
                    residual_model = BootstrapRidge(
                        self.alpha, self.n_bootstrap, self.residual_min_samples,
                        seed=self._seed_for(("residual", requirement_id, sibling)),
                    ).fit(X, residual_y)
                # Prefer the sibling with the most paired observations (more
                # correction evidence), falling back to any ready sibling at
                # all when every candidate has zero paired observations.
                if best is None or len(y) > best[2]:
                    best = (sibling, residual_model, len(y))
            if best is not None:
                self.residual_models[requirement_id] = (best[0], best[1])

    def predict_log_nominal(self, requirement_id, X):
        """(mean, std, source) in log-nominal space, or None if unmodeled.
        `source` is "own", "corrected" (prior sibling plus a fitted
        residual), or "prior" (sibling only, own data still too thin or
        entirely absent — including before this requirement's first
        observation of any kind).
        """
        own = self.own_models.get(requirement_id)
        if own and own.ready:
            mean, std = own.predict(X)
            return mean, std, "own"
        if requirement_id in self.residual_models:
            sibling, residual_model = self.residual_models[requirement_id]
            sibling_model = self.own_models.get(sibling)
            if sibling_model and sibling_model.ready:
                sibling_mean, sibling_std = sibling_model.predict(X)
                if residual_model is not None and residual_model.ready:
                    residual_mean, residual_std = residual_model.predict(X)
                    mean = sibling_mean + residual_mean
                    std = np.sqrt(sibling_std**2 + residual_std**2)
                    return mean, std, "corrected"
                # Sibling data only: still usable, but extrapolating a
                # cheaper fidelity to a different requirement deserves
                # extra distrust, reflected as inflated spread.
                return sibling_mean, sibling_std * 1.5, "prior"
        return None

    def predict_margin(self, requirement_id, X):
        """(mean_margin, std_margin, source) or None."""
        direction = self.directions.get(requirement_id)
        limit = self.limits.get(requirement_id)
        if direction is None or limit is None:
            return None
        prediction = self.predict_log_nominal(requirement_id, X)
        if prediction is None:
            return None
        mean_log, std_log, source = prediction
        nominal_mean = np.exp(mean_log)
        # Delta method: d(nominal)/d(log nominal) = nominal.
        nominal_std = np.abs(nominal_mean * std_log)
        margin_mean = (limit - nominal_mean) if direction == 1 else (nominal_mean - limit)
        return margin_mean, nominal_std, source

    def sensitivity_to_thickness(self, requirement_id, layout):
        """d(log nominal)/d(thickness_cm) per material, from whichever
        model (own, or sibling-plus-residual) currently predicts this
        requirement. None where no model exists yet.
        """
        own = self.own_models.get(requirement_id)
        coefficients = None
        if own and own.ready:
            coefficients = own.point_model.raw_coefficients()
        elif requirement_id in self.residual_models:
            sibling, residual_model = self.residual_models[requirement_id]
            sibling_model = self.own_models.get(sibling)
            if sibling_model and sibling_model.ready:
                coefficients = sibling_model.point_model.raw_coefficients()
                if residual_model is not None and residual_model.ready:
                    coefficients = coefficients + residual_model.point_model.raw_coefficients()
        if coefficients is None:
            return None
        return {
            material: float(coefficients[layout.thickness_feature_index(material)])
            for material in layout.material_names
        }

    def dose_requirement_ids(self):
        """Every requirement whose unit is a dose rate and which has, at
        least once, been decided by a bounded rule. This is how "the
        primary dose metric" is found without assuming its id: CASE-001
        has one (SHIELD-R2-transport); the coupled case is expected to
        have two (neutron and photon), and both are picked up the same
        way.
        """
        ids = []
        for requirement_id, unit in self.units.items():
            if unit_role(unit) != "dose_rate":
                continue
            if any(prefix == "bounded." for prefix in self.rules_seen.get(requirement_id, ())):
                ids.append(requirement_id)
        return sorted(ids)

    def mass_requirement_ids(self):
        return sorted(rid for rid, unit in self.units.items() if unit_role(unit) == "mass")

    def length_requirement_ids(self):
        return sorted(rid for rid, unit in self.units.items() if unit_role(unit) == "length")


def score_pool(bank, requirement_ids, features, beta):
    """Predicted worst-case margin plus an exploration bonus, jointly over
    whichever requirements currently have a usable model. Returns
    `(score, worst_margin_mean, binding_requirement_ids, used_requirement_ids)`
    per pool row, or None if no requirement is modeled yet at all.
    """
    means, stds, used = [], [], []
    for requirement_id in requirement_ids:
        prediction = bank.predict_margin(requirement_id, features)
        if prediction is None:
            continue
        mean, std, _source = prediction
        means.append(mean)
        stds.append(std)
        used.append(requirement_id)
    if not used:
        return None
    means = np.stack(means)  # (R, N)
    stds = np.stack(stds)
    worst_index = means.argmin(axis=0)
    worst_mean = means[worst_index, np.arange(means.shape[1])]
    worst_std = stds[worst_index, np.arange(means.shape[1])]
    score = worst_mean + beta * worst_std
    binding = [used[i] for i in worst_index]
    return score, worst_mean, binding, used


def score_screened_candidates(bank, dataset, requirement_ids, candidate_ids, beta):
    """Rank already-screened candidates for finalist selection. Unlike
    `score_pool` (used to propose *unscreened* candidates, where nothing
    is known yet and every number must come from the surrogate), a
    screened candidate already carries Core's own exact margin for every
    requirement its steps so far have actually evaluated -- most often
    mass and thickness, which are deterministic and have no uncertainty
    left for a transport call to resolve. Using the surrogate for those
    instead of the real number both throws away a fact Core already
    established and can point the exploration bonus at a requirement
    transport cannot inform at all. So: the real observed margin (std 0,
    nothing to explore) wherever Core has already reported one for that
    exact candidate; the surrogate's prediction only for a requirement
    that candidate has not yet been evaluated on (transport, typically).
    Returns `(score, worst_margin_mean, binding_requirement_ids)` arrays
    aligned with `candidate_ids`, or None if not one candidate has any
    usable number at all.
    """
    scores, worst_means, binding = [], [], []
    any_usable = False
    for candidate_id in candidate_ids:
        record = dataset.records[candidate_id]
        best_requirement, best_mean, best_std = None, None, None
        for requirement_id in requirement_ids:
            data = record["requirements"].get(requirement_id)
            if data and data.get("status") != STATUS_NOT_EVALUATED and data.get("margin") is not None:
                mean, std = data["margin"], 0.0
            else:
                prediction = bank.predict_margin(requirement_id, record["features"][None, :])
                if prediction is None:
                    continue
                mean, std = float(prediction[0][0]), float(prediction[1][0])
            if best_mean is None or mean < best_mean:
                best_requirement, best_mean, best_std = requirement_id, mean, std
        if best_requirement is None:
            scores.append(-math.inf)
            worst_means.append(None)
            binding.append(None)
            continue
        any_usable = True
        scores.append(best_mean + beta * best_std)
        worst_means.append(best_mean)
        binding.append(best_requirement)
    if not any_usable:
        return None
    return np.array(scores), worst_means, binding


# ----------------------------------------------------------------------
# Candidate generation: fresh random layer stacks and local mutations of
# promising ones, all on the configurable thickness grid.
# ----------------------------------------------------------------------


def format_thickness(value_cm):
    return core.canonical_decimal(Decimal(str(value_cm)))


def random_layers(rng, material_names, grid_cm, max_layers, max_total_cm):
    max_steps = max(1, int(Decimal(str(max_total_cm)) // Decimal(str(grid_cm))))
    layer_count = int(rng.integers(1, max_layers + 1))
    layer_count = min(layer_count, max_steps)
    total_steps = int(rng.integers(layer_count, max_steps + 1))
    if layer_count == 1:
        cuts = []
    else:
        cuts = sorted(rng.choice(np.arange(1, total_steps), size=layer_count - 1, replace=False).tolist())
    bounds = [0, *cuts, total_steps]
    steps = [bounds[i + 1] - bounds[i] for i in range(layer_count)]
    materials = [material_names[int(rng.integers(0, len(material_names)))] for _ in range(layer_count)]
    grid = Decimal(str(grid_cm))
    return [
        {"material": m, "thickness_cm": format_thickness(grid * s)}
        for m, s in zip(materials, steps)
    ]


def mutate_layers(rng, layers, material_names, grid_cm, max_layers, max_total_cm):
    layers = copy.deepcopy(layers)
    grid = Decimal(str(grid_cm))
    action = rng.choice(["thickness", "material", "add", "remove"])
    if action == "thickness" or len(layers) == 0:
        if layers:
            i = int(rng.integers(0, len(layers)))
            current = Decimal(layers[i]["thickness_cm"])
            delta = grid if rng.random() < 0.5 else -grid
            new_value = current + delta
            total_others = sum(Decimal(l["thickness_cm"]) for j, l in enumerate(layers) if j != i)
            if new_value >= grid and (total_others + new_value) <= Decimal(str(max_total_cm)):
                layers[i]["thickness_cm"] = format_thickness(new_value)
    elif action == "material":
        i = int(rng.integers(0, len(layers)))
        layers[i]["material"] = material_names[int(rng.integers(0, len(material_names)))]
    elif action == "add" and len(layers) < max_layers:
        total = sum(Decimal(l["thickness_cm"]) for l in layers)
        if total + grid <= Decimal(str(max_total_cm)):
            position = int(rng.integers(0, len(layers) + 1))
            new_layer = {
                "material": material_names[int(rng.integers(0, len(material_names)))],
                "thickness_cm": format_thickness(grid),
            }
            layers.insert(position, new_layer)
    elif action == "remove" and len(layers) > 1:
        i = int(rng.integers(0, len(layers)))
        del layers[i]
    if not layers:
        return random_layers(rng, material_names, grid_cm, max_layers, max_total_cm)
    return layers


def canonical_layers(layers):
    """Merge adjacent layers of the same material and drop zero-thickness
    layers, so that "20 cm polyethylene + 70 cm polyethylene" and "90 cm
    polyethylene" are one design. Transport, activation, mass, and thickness
    cannot tell them apart, and the recovery campaign spent five of eight
    transports learning that the hard way."""
    merged = []
    for layer in layers:
        thickness = Decimal(str(layer["thickness_cm"]))
        if thickness <= 0:
            continue
        if merged and merged[-1]["material"] == layer["material"]:
            merged[-1]["thickness_cm"] = format_thickness(Decimal(merged[-1]["thickness_cm"]) + thickness)
        else:
            merged.append({"material": layer["material"], "thickness_cm": format_thickness(thickness)})
    return merged


def estimated_mass_kg(layers, materials_table, area_cm2):
    total_g = Decimal(0)
    for layer in layers:
        density = Decimal(materials_table[layer["material"]]["density_g_cm3"])
        total_g += density * Decimal(str(layer["thickness_cm"])) * Decimal(str(area_cm2))
    return float(total_g / Decimal(1000))


def propose_pool(rng, size, material_names, grid_cm, max_layers, max_total_cm,
                  materials_table, area_cm2, mass_limit_kg, history, exploit_fraction=0.5):
    """`history`: list of layer-stacks to mutate from (best-so-far), may be empty."""
    pool = []
    attempts = 0
    max_attempts = size * 20
    while len(pool) < size and attempts < max_attempts:
        attempts += 1
        if history and rng.random() < exploit_fraction:
            base = history[int(rng.integers(0, len(history)))]
            layers = mutate_layers(rng, base, material_names, grid_cm, max_layers, max_total_cm)
        else:
            layers = random_layers(rng, material_names, grid_cm, max_layers, max_total_cm)
        layers = canonical_layers(layers)
        if not layers:
            continue
        if mass_limit_kg is not None and area_cm2 is not None:
            if estimated_mass_kg(layers, materials_table, area_cm2) > mass_limit_kg:
                continue
        pool.append(layers)
    return pool


# ----------------------------------------------------------------------
# Prior-log seeding: before round 1, load every row of each prior
# campaign's --log file whose verdict requirement ids match the current
# contract's, adding it to the dataset exactly as a live report would be.
# The archived logs under examples/cases/.../campaign-rev1/ and
# campaign-rev2/ record absolute workspace paths that no longer exist in a
# fresh checkout; their candidates/ directories were archived beside the
# logs precisely so this fallback can find the design anyway.
# ----------------------------------------------------------------------


def row_was_transported(verdicts):
    """Whether a campaign-log row's own `verdicts` (or a report's
    `margins`, same shape) carry a real bounded dose-rate result -- i.e.
    transport actually ran for this exact call, as opposed to a
    screen-only call where the dose-rate requirements are
    `not_evaluated`. Found by unit and rule prefix, exactly as
    `SurrogateBank.dose_requirement_ids` finds "the primary dose metric",
    never by a hard-coded step name or requirement id.
    """
    for entry in verdicts:
        if entry.get("status") == STATUS_NOT_EVALUATED:
            continue
        if core.is_bounded_rule(entry.get("rule")) and unit_role(entry.get("unit")) == "dose_rate":
            return True
    return False


def _prior_candidate_path(row, log_path):
    """The on-disk path of a prior-log row's candidate: the recorded
    `supplied_inputs[].path` if it still exists, else `<directory of the
    log>/candidates/<basename>`. None if the row has no candidate input,
    or neither path exists.
    """
    supplied = next(
        (s for s in row.get("supplied_inputs", []) if s.get("input_id") == "candidate"),
        None,
    )
    if not supplied or not supplied.get("path"):
        return None
    recorded_path = Path(supplied["path"])
    if recorded_path.exists():
        return recorded_path
    fallback_path = Path(log_path).parent / "candidates" / recorded_path.name
    if fallback_path.exists():
        return fallback_path
    return None


def resolve_prior_candidate(row, log_path):
    """`(candidate_id, canonical layers)` for a prior-log row, or None if
    the candidate cannot be resolved or is unusable (unreadable, missing,
    or empty layers). Never raises: any I/O or shape problem is "cannot be
    resolved", for the caller to skip and count.
    """
    path = _prior_candidate_path(row, log_path)
    if path is None:
        return None
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    layers = canonical_layers(data.get("layers") or [])
    if not layers:
        return None
    candidate_id = data.get("candidate_id") or Path(path).stem
    return candidate_id, layers


def seed_prior_logs(dataset, prior_log_paths, current_requirement_ids):
    """Seed `dataset` with every row of each `prior_log_paths` campaign log
    whose verdict requirement ids equal `current_requirement_ids` (the
    live contract's own set, read from the bootstrap run's report -- never
    assumed). A row from a different contract shape is skipped and
    counted, not guessed at; so is one whose candidate cannot be resolved.
    Every resolved row is added to `dataset` exactly as a live report
    would be -- the surrogate trains on it like any other observation --
    merging multiple rows of the same original candidate_id within one
    log (a screen call, then later a transport call) into one record,
    exactly as Core's own semantics do; a candidate_id is namespaced by
    which prior log it came from so two arms that both start their own
    candidates at "c-0000" cannot collide.

    Returns `(stats, transported_signatures)`. `stats` is
    `{"logs", "seeded", "transported", "skipped"}`. `transported_signatures`
    is the set of canonical layer signatures a prior log already sent
    through real transport (a bounded dose-rate result, not
    `not_evaluated`), for finalist selection to avoid re-spending budget on.
    """
    stats = {"logs": 0, "seeded": 0, "transported": 0, "skipped": 0}
    transported_signatures = set()
    for log_index, raw_path in enumerate(prior_log_paths):
        log_path = Path(raw_path)
        if not log_path.exists():
            raise SystemExit(f"--prior-log {log_path}: file does not exist")
        stats["logs"] += 1
        for row in core.read_jsonl(log_path):
            verdicts = row.get("verdicts", [])
            row_ids = {v["requirement_id"] for v in verdicts if v.get("requirement_id")}
            if row_ids != current_requirement_ids:
                stats["skipped"] += 1
                continue
            resolved = resolve_prior_candidate(row, log_path)
            if resolved is None:
                stats["skipped"] += 1
                continue
            original_id, layers = resolved
            candidate = {"candidate_id": f"prior-{log_index}-{original_id}", "layers": layers}
            report = {"margins": verdicts, "campaign": {"verdicts": []}}
            try:
                dataset.add_report(candidate, report)
            except (KeyError, ValueError) as exc:
                core.eprint(f"--prior-log {log_path}: skipping a row, could not build features ({exc})")
                stats["skipped"] += 1
                continue
            stats["seeded"] += 1
            if row_was_transported(verdicts):
                stats["transported"] += 1
                transported_signatures.add(layer_signature(layers))
    return stats, transported_signatures


# ----------------------------------------------------------------------
# Real (not predicted) bookkeeping straight from Core's margins: used for
# the stopping rule and for what "feasible" and "binding" mean.
# ----------------------------------------------------------------------


def stopping_decision(best_so_far, rounds_since_improvement, current_best, tolerance=1e-9):
    """The "no improvement" half of the stopping rule (the other half,
    budget exhaustion, is the caller's loop bound). `current_best` is this
    round's best observed worst-case *real* margin (None if nothing was
    evaluated this round). Returns `(new_best_so_far,
    new_rounds_since_improvement, improved)`. Pure and Core-independent so
    it is directly testable without running a campaign.
    """
    if current_best is not None and (best_so_far is None or current_best > best_so_far + tolerance):
        return current_best, 0, True
    return best_so_far, rounds_since_improvement + 1, False


def transport_stopping_decision(best_so_far, transports_since_improvement, transport_margins, tolerance=1e-9):
    """The transport-counted half of the stopping rule: fold this round's
    sequence of newly-transported worst-case *real* margins (in the order
    they were transported -- there may be several, now that a round sends
    `--finalists-per-round` candidates to transport at once) one at a
    time, so that a round with several non-improving transports costs
    several units of patience, not one. A `None` margin (nothing usable
    from that transport) is ignored. Returns `(new_best_so_far,
    new_transports_since_improvement)`. Pure and Core-independent, like
    `stopping_decision`, and meant to be folded across rounds the same way:
    the caller passes this call's return values back in as the next one's
    `best_so_far`/`transports_since_improvement`.
    """
    for margin_value in transport_margins:
        if margin_value is None:
            continue
        if best_so_far is None or margin_value > best_so_far + tolerance:
            best_so_far = margin_value
            transports_since_improvement = 0
        else:
            transports_since_improvement += 1
    return best_so_far, transports_since_improvement


def patience_exhausted(transport_calls, rounds_since_improvement, patience_rounds,
                        transports_since_improvement, patience_transports):
    """Which half of the patience rule governs stopping: round-based
    before any transport has run, transport-based once at least one has
    -- a discrete switch, not an OR of both, because the screen-level
    margin the round-based rule watches saturates long before the
    bounded requirements do (revision 2's finding). Pure and directly
    testable, like `stopping_decision`; budget exhaustion is a separate
    condition in the caller's loop.
    """
    if transport_calls > 0:
        return transports_since_improvement >= patience_transports
    return rounds_since_improvement >= patience_rounds


def worst_real_margin(report):
    """min margin over requirements Core actually evaluated (status !=
    not_evaluated) this run, or None if nothing was evaluated."""
    values = [
        float(Fraction(entry["margin"]))
        for entry in core.all_margins(report)
        if entry["status"] != STATUS_NOT_EVALUATED and entry.get("margin") is not None
    ]
    return min(values) if values else None


def is_fully_feasible(report):
    entries = core.all_margins(report)
    if not entries:
        return False
    return all(entry["status"] == STATUS_PASS for entry in entries)


# ----------------------------------------------------------------------
# Constellation: a pure function of a parsed campaign log (plus the
# feature layout/materials table, needed only for sensitivity), so it can
# run standalone over any --log file, live or archived.
# ----------------------------------------------------------------------


def build_constellation(log_rows, bank=None, layout=None, tolerance=1e-9):
    requirement_stats = {}
    for row in log_rows:
        for entry in row.get("verdicts", []):
            requirement_id = entry["requirement_id"]
            stats = requirement_stats.setdefault(
                requirement_id,
                {
                    "pass": 0, "fail": 0, "inconclusive": 0, "not_evaluated": 0,
                    "binding": 0, "unit": None,
                },
            )
            status = entry["status"]
            if status in stats:
                stats[status] += 1
            if entry.get("unit"):
                stats["unit"] = entry["unit"]
        numeric = [
            (entry["requirement_id"], float(Fraction(entry["margin"])))
            for entry in row.get("verdicts", [])
            if entry["status"] != STATUS_NOT_EVALUATED and entry.get("margin") is not None
        ]
        if numeric:
            worst = min(value for _rid, value in numeric)
            for requirement_id, value in numeric:
                if value <= worst + tolerance:
                    requirement_stats[requirement_id]["binding"] += 1

    dose_ids = bank.dose_requirement_ids() if bank else []
    mass_ids = bank.mass_requirement_ids() if bank else []

    feasible_rows = []
    for row in log_rows:
        entries = row.get("verdicts", [])
        if entries and all(entry["status"] == STATUS_PASS for entry in entries):
            by_id = {entry["requirement_id"]: entry for entry in entries}
            dose_values = []
            ok = True
            for rid in dose_ids:
                entry = by_id.get(rid)
                if not entry or entry.get("nominal") is None:
                    ok = False
                    break
                dose_values.append(float(Fraction(entry["nominal"])))
            mass_value = None
            for rid in mass_ids:
                entry = by_id.get(rid)
                if entry and entry.get("nominal") is not None:
                    mass_value = float(Fraction(entry["nominal"]))
                    break
            if ok and dose_values:
                feasible_rows.append(
                    {
                        "candidate_sha256": _candidate_sha(row),
                        "primary_dose": max(dose_values),
                        "mass": mass_value,
                    }
                )

    pareto = pareto_front(feasible_rows) if feasible_rows else []
    best = sorted(feasible_rows, key=lambda r: (r["primary_dose"], r["mass"] if r["mass"] is not None else math.inf))[:5]

    sensitivity = {}
    if bank is not None and layout is not None:
        for requirement_id in requirement_stats:
            values = bank.sensitivity_to_thickness(requirement_id, layout)
            if values is not None:
                sensitivity[requirement_id] = values

    return {
        "schema": "avila.shielding/constellation/v1",
        "notice": SURROGATE_NOTICE,
        "requirements": requirement_stats,
        "dose_requirement_ids": dose_ids,
        "mass_requirement_ids": mass_ids,
        "sensitivity_d_log_nominal_d_thickness_cm": sensitivity,
        "pareto_mass_vs_primary_dose": pareto,
        "best_feasible_candidates": best,
        "rows_considered": len(log_rows),
    }


def _candidate_sha(row):
    for supplied in row.get("supplied_inputs", []):
        if supplied.get("input_id") == "candidate":
            return supplied.get("sha256")
    return None


def pareto_front(rows):
    """Non-dominated rows minimizing (mass, primary_dose)."""
    front = []
    for row in rows:
        if row["mass"] is None:
            continue
        dominated = any(
            other is not row
            and other["mass"] is not None
            and other["mass"] <= row["mass"]
            and other["primary_dose"] <= row["primary_dose"]
            and (other["mass"] < row["mass"] or other["primary_dose"] < row["primary_dose"])
            for other in rows
        )
        if not dominated:
            front.append(row)
    return sorted(front, key=lambda r: r["mass"])


# ----------------------------------------------------------------------
# The search loop.
# ----------------------------------------------------------------------


def discover_bounds(run, materials_table, material_names, grid_cm, args, log):
    """One tiny screen-only run to learn the mass and thickness limits Core
    actually enforces, by unit, rather than assuming their values."""
    candidate = core.write_candidate(
        Path(args.out) / "candidates" / "bootstrap.json",
        "bootstrap",
        [(material_names[0], format_thickness(Decimal(str(grid_cm))))],
    )
    report = run(candidate, transport=False)
    limits = core.discover_limits(report)
    return candidate, report, limits["mass"], limits["length"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core", default="target/debug/avila-core")
    parser.add_argument("--case", default="examples/cases/case-001-shield-search")
    parser.add_argument("--shielding", default="examples/capabilities/shielding")
    parser.add_argument("--agents", default="examples/agents")
    parser.add_argument("--nuclear-data", required=True)
    parser.add_argument("--cross-sections", required=True, help="value for OPENMC_CROSS_SECTIONS")
    parser.add_argument("--python3", default="/usr/bin/python3")
    parser.add_argument("--openmc-python", required=True)
    parser.add_argument("--out", default="workspaces/shield-search2")
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--grid-cm", type=float, default=5)
    parser.add_argument("--max-layers", type=int, default=3,
                         help="matches the transport qualification envelope's layer limit")
    parser.add_argument("--screen-budget", type=int, default=200)
    parser.add_argument("--transport-budget", type=int, default=6)
    parser.add_argument("--batch-size", type=int, default=20)
    parser.add_argument("--pool-multiplier", type=int, default=4)
    parser.add_argument("--finalists-per-round", type=int, default=3)
    parser.add_argument("--patience", type=int, default=5,
                         help="before the first transport: rounds with no improvement in the best observed "
                              "worst-case margin before stopping")
    parser.add_argument("--patience-transports", type=int, default=10,
                         help="once any transport has run: consecutive transports with no improvement in the "
                              "best transported worst-case real margin before stopping")
    parser.add_argument("--alpha", type=float, default=2.0, help="ridge regularization strength")
    parser.add_argument("--bootstrap", type=int, default=16, help="bootstrap ensemble size")
    parser.add_argument("--min-samples", type=int, default=5, help="observations before a requirement gets its own model")
    parser.add_argument("--beta", type=float, default=1.0, help="exploration-bonus weight on bootstrap spread")
    parser.add_argument("--expect-manifest", default=None, metavar="SHA256",
                         help="refuse any run whose package manifest digest differs from this pinned value")
    parser.add_argument("--source-root", action="append", default=[], metavar="NAME=PATH",
                         help="additional or overriding source root passed to Core (repeatable), for cases that bind more roots than CASE-001")
    parser.add_argument("--materials", nargs="*", default=None,
                         help="restrict the search to these materials from the table (default: every material)")
    parser.add_argument("--random", action="store_true",
                         help="random-search arm: propose uniformly at random, never mutate the best-so-far, and pick finalists by observed screen margin instead of the surrogate")
    parser.add_argument("--prior-log", action="append", default=[], metavar="FILE",
                         help="seed the dataset from a prior campaign's --log file before round 1 (repeatable); "
                              "rows whose requirement ids differ from this run's contract, or whose candidate "
                              "cannot be resolved, are skipped and counted")
    parser.add_argument("--retransport-prior", action="store_true",
                         help="allow finalist selection to send a design to transport even if a --prior-log "
                              "already transported its canonical layer signature (default: it will not)")
    args = parser.parse_args()

    out = Path(args.out)
    (out / "candidates").mkdir(parents=True, exist_ok=True)
    log = out / "campaign-log.jsonl"

    materials_table = core.load_materials(Path(args.shielding) / "materials.json")
    material_names = sorted(materials_table)
    if args.materials:
        unknown = sorted(set(args.materials) - set(material_names))
        if unknown:
            raise SystemExit(f"unknown material(s) {unknown}; choose from {material_names}")
        material_names = sorted(set(args.materials))
    source = core.load_source(Path(args.shielding) / "source.json")
    area_cm2 = float(source["area_cm2"]) if "area_cm2" in source else None
    layout = FeatureLayout(sorted(materials_table))
    rng = np.random.default_rng(args.seed)

    screen_capabilities = {"python3": args.python3}
    transport_capabilities = {"python3": args.python3, "openmc-python": args.openmc_python}
    transport_environment = {"OPENMC_CROSS_SECTIONS": args.cross_sections}
    source_roots = {
        "case": args.case, "shielding": args.shielding, "agents": args.agents,
        "nuclear-data": args.nuclear_data,
    }
    for spec in args.source_root:
        name, sep, path = spec.partition("=")
        if not sep or not name or not path:
            raise SystemExit(f"--source-root expects NAME=PATH, got {spec!r}")
        source_roots[name] = path

    def run(candidate, transport):
        return core.run_core(
            args.core, args.case, out / "candidates" / f"{candidate['candidate_id']}.json",
            source_roots=source_roots,
            capabilities=transport_capabilities if transport else screen_capabilities,
            environment=transport_environment if transport else None,
            log=log, extra_args=(['--expect-manifest', args.expect_manifest] if getattr(args, 'expect_manifest', None) else None),
        )

    core.eprint("discovering mass/thickness bounds from a bootstrap screen run...")
    _bootstrap_candidate, _bootstrap_report, mass_limit_kg, thickness_limit_cm = discover_bounds(
        run, materials_table, material_names, args.grid_cm, args, log
    )
    if thickness_limit_cm is None:
        thickness_limit_cm = args.grid_cm * args.max_layers * 20
        core.eprint(f"WARNING: no length-unit requirement found; using generous default {thickness_limit_cm} cm")
    else:
        core.eprint(f"discovered thickness bound: {thickness_limit_cm} cm")
    if mass_limit_kg is not None:
        core.eprint(f"discovered mass bound: {mass_limit_kg} kg")

    # The current contract's own requirement-id set, from the bootstrap
    # run's own report -- never assumed -- is the yardstick prior-log
    # seeding uses to recognize "this row belongs to the same contract
    # shape", before the dataset has any live requirement_specs of its own.
    current_requirement_ids = {
        entry["requirement_id"] for entry in core.all_margins(_bootstrap_report) if entry.get("requirement_id")
    }

    dataset = CampaignDataset(layout, materials_table)
    seed_stats = {"logs": 0, "seeded": 0, "transported": 0, "skipped": 0}
    prior_transported_signatures = set()
    if args.prior_log:
        seed_stats, prior_transported_signatures = seed_prior_logs(
            dataset, args.prior_log, current_requirement_ids
        )
        core.eprint(
            f"prior-log seeding: {seed_stats['seeded']} prior observations "
            f"({seed_stats['transported']} transported) from {seed_stats['logs']} logs; "
            f"{seed_stats['skipped']} rows skipped"
        )

    bank = SurrogateBank(dataset, alpha=args.alpha, n_bootstrap=args.bootstrap,
                          min_samples=args.min_samples, seed=args.seed)
    bank.refit()  # a no-op on an empty dataset; on a seeded one, round 1's
    # own proposal ranking (not just its finalist selection, refit after
    # screening) already benefits from the prior campaigns' observations.

    screened = []       # every screened candidate's bookkeeping, in order
    transported_ids = set()
    finalists = []
    best_worst_margin = None
    rounds_since_improvement = 0
    best_transported_margin = None
    transports_since_improvement = 0
    screen_calls = 0
    transport_calls = 0
    round_index = 0

    while screen_calls < args.screen_budget and not patience_exhausted(
        transport_calls, rounds_since_improvement, args.patience,
        transports_since_improvement, args.patience_transports,
    ):
        round_index += 1
        remaining = args.screen_budget - screen_calls
        batch_size = min(args.batch_size, remaining)
        history = [record["layers"] for record in dataset.records.values()]
        # Favor mutating the best-so-far (by worst *real* margin) so local
        # search concentrates near what has already worked.
        history_ranked = sorted(
            history,
            key=lambda layers: -(
                next(
                    (s["worst_real_margin"] for s in screened if s["layers"] == layers and s["worst_real_margin"] is not None),
                    -math.inf,
                )
            ),
        )[:10]
        pool = propose_pool(
            rng, batch_size * args.pool_multiplier, material_names, args.grid_cm,
            args.max_layers, thickness_limit_cm, materials_table, area_cm2, mass_limit_kg,
            [] if args.random else history_ranked,
            exploit_fraction=0.0 if args.random else 0.5,
        )
        known_signatures = {layer_signature(layers) for layers in history}
        pool = [layers for layers in pool if layer_signature(layers) not in known_signatures]
        # de-duplicate within the pool itself
        seen_in_pool = set()
        deduped_pool = []
        for layers in pool:
            signature = layer_signature(layers)
            if signature in seen_in_pool:
                continue
            seen_in_pool.add(signature)
            deduped_pool.append(layers)
        pool = deduped_pool

        if not pool:
            core.eprint(f"round {round_index}: candidate pool exhausted (grid saturated); stopping")
            break

        features = np.stack([layout.vector(layers, materials_table) for layers in pool])
        requirement_ids = sorted(dataset.requirement_ids()) or ["__cold_start__"]
        scored = None if args.random else score_pool(bank, requirement_ids, features, args.beta)
        if scored is None:
            # Cold start (no surrogate exists yet) or the random arm: screen in
            # proposal order, which for the random arm is uniform sampling.
            chosen = list(range(min(batch_size, len(pool))))
        else:
            score, _worst_mean, _binding, _used = scored
            chosen = list(np.argsort(-score)[:batch_size])

        for i in chosen:
            layers = pool[i]
            index = len(screened)
            candidate_id = f"c-{index:04d}"
            candidate = core.write_candidate(out / "candidates" / f"{candidate_id}.json", candidate_id, [
                (layer["material"], layer["thickness_cm"]) for layer in layers
            ])
            report = run(candidate, transport=False)
            screen_calls += 1
            dataset.add_report(candidate, report)
            wrm = worst_real_margin(report)
            screened.append({
                "id": candidate_id, "layers": layers, "worst_real_margin": wrm,
                "fully_feasible": is_fully_feasible(report),
            })
            core.eprint(
                f"round {round_index} [{screen_calls}/{args.screen_budget}] {candidate_id}: "
                f"worst_real_margin={wrm if wrm is None else round(wrm, 4)}"
            )
            if screen_calls >= args.screen_budget:
                break

        bank.refit()

        # Finalist selection: among screened candidates never yet
        # transported, rank by worst-case margin over every requirement
        # Core currently reports -- Core's own exact number wherever this
        # candidate already has one (mass and thickness, almost always;
        # transport has nothing to add there), the surrogate's prediction
        # (screen- and transport-fed alike, via SurrogateBank's sibling
        # correction) only for what has not been evaluated for it yet.
        candidates_for_transport = [
            s for s in screened
            if s["id"] not in transported_ids
            and (args.retransport_prior or layer_signature(s["layers"]) not in prior_transported_signatures)
        ]
        just_transported_margins = []
        if candidates_for_transport and transport_calls < args.transport_budget:
            requirement_ids = sorted(dataset.requirement_ids())
            candidate_ids = [s["id"] for s in candidates_for_transport]
            scored = None if args.random else score_screened_candidates(bank, dataset, requirement_ids, candidate_ids, args.beta)
            if scored is not None:
                score, _worst_mean, _binding = scored
                order = list(np.argsort(-score))
            else:
                order = sorted(
                    range(len(candidates_for_transport)),
                    key=lambda i: -(candidates_for_transport[i]["worst_real_margin"] or -math.inf),
                )
            take = min(args.finalists_per_round, args.transport_budget - transport_calls, len(order))
            for i in order[:take]:
                entry = candidates_for_transport[i]
                candidate_path = out / "candidates" / f"{entry['id']}.json"
                candidate = json.loads(candidate_path.read_text())
                report = run(candidate, transport=True)
                transport_calls += 1
                transported_ids.add(entry["id"])
                dataset.add_report(candidate, report)
                wrm = worst_real_margin(report)
                entry["worst_real_margin"] = wrm
                entry["fully_feasible"] = is_fully_feasible(report)
                finalists.append({**entry, "report": report})
                just_transported_margins.append(wrm)
                core.eprint(
                    f"round {round_index}: transported {entry['id']}: "
                    f"worst_real_margin={wrm if wrm is None else round(wrm, 4)}, "
                    f"feasible={entry['fully_feasible']}"
                )
            bank.refit()

        # Before the first transport, the stopping rule follows the
        # screened candidates' best real margin, counted in rounds. Once
        # any transport has run, it switches (see `patience_exhausted`) to
        # this round's newly-transported real margins, counted one
        # transport at a time -- the screen's margin saturates long before
        # the bounded requirements do, and stopping on it (or on it only
        # once per round) left budget unspent.
        stop_population = (
            [s for s in screened if s["id"] in transported_ids] if transported_ids else screened
        )
        current_best = max(
            (s["worst_real_margin"] for s in stop_population if s["worst_real_margin"] is not None),
            default=None,
        )
        best_worst_margin, rounds_since_improvement, _improved = stopping_decision(
            best_worst_margin, rounds_since_improvement, current_best
        )
        best_transported_margin, transports_since_improvement = transport_stopping_decision(
            best_transported_margin, transports_since_improvement, just_transported_margins
        )
        core.eprint(
            f"round {round_index} done: best_worst_margin={best_worst_margin}, "
            f"rounds_since_improvement={rounds_since_improvement}, "
            f"best_transported_margin={best_transported_margin}, "
            f"transports_since_improvement={transports_since_improvement}, "
            f"screen_calls={screen_calls}, transport_calls={transport_calls}"
        )

    if screen_calls >= args.screen_budget:
        stop_reason = "screen budget exhausted"
    elif transport_calls > 0:
        stop_reason = (
            f"{args.patience_transports} consecutive transports with no improvement in the "
            "best transported worst-case real margin"
        )
    else:
        stop_reason = f"no improvement in best worst-case margin for {args.patience} rounds"
    core.eprint(f"stopping: {stop_reason}")

    # ---- Outputs -------------------------------------------------
    feasible_finalists = [f for f in finalists if f["fully_feasible"]]
    seeding_sentence = (
        f"Seeded with {seed_stats['seeded']} prior observations ({seed_stats['transported']} transported) "
        f"from {seed_stats['logs']} logs; {seed_stats['skipped']} rows skipped."
    )
    lines = [
        "# Surrogate-assisted shielding configuration search", "",
        f"{screen_calls} candidates screened over {round_index} round(s); "
        f"{transport_calls} sent to transport; stopped because {stop_reason}.",
        seeding_sentence,
        "",
        "| candidate | layers | worst real margin | fully feasible |",
        "| --- | --- | ---: | --- |",
    ]
    for entry in finalists:
        layer_text = " + ".join(f"{l['thickness_cm']} cm {l['material']}" for l in entry["layers"])
        wrm = entry["worst_real_margin"]
        lines.append(f"| {entry['id']} | {layer_text} | {core.show(str(wrm)) if wrm is not None else '-'} | {entry['fully_feasible']} |")
    lines += [
        "",
        f"Transport ran on {len(finalists)} finalist(s); {len(feasible_finalists)} passed every requirement Core evaluated.",
        "An `inconclusive` verdict means the statistical interval straddles the limit; a `not_evaluated` one means the "
        "requirement's evidence was withheld, out of its qualification envelope, or never run — never a hidden PASS or FAIL.",
        "",
        SURROGATE_NOTICE,
    ]
    (out / "summary.md").write_text("\n".join(lines) + "\n")
    print((out / "summary.md").read_text())

    log_rows = core.read_jsonl(log)
    constellation = build_constellation(log_rows, bank=bank, layout=layout)
    (out / "constellation.json").write_text(json.dumps(constellation, indent=2, default=str) + "\n")

    md = ["# Engineering constellation", "", SURROGATE_NOTICE, "", seeding_sentence, "",
          "## Requirement states", "",
          "| requirement | pass | fail | inconclusive | not_evaluated | times binding |",
          "| --- | ---: | ---: | ---: | ---: | ---: |"]
    for requirement_id, stats in sorted(constellation["requirements"].items()):
        md.append(
            f"| {requirement_id} | {stats['pass']} | {stats['fail']} | {stats['inconclusive']} | "
            f"{stats['not_evaluated']} | {stats['binding']} |"
        )
    md += ["", "## Sensitivity: d(log nominal)/d(thickness_cm) by material", ""]
    for requirement_id, sens in sorted(constellation["sensitivity_d_log_nominal_d_thickness_cm"].items()):
        parts = ", ".join(f"{material}={value:+.4f}" for material, value in sorted(sens.items()))
        md.append(f"- **{requirement_id}**: {parts}")
    md += ["", "## Pareto set: mass vs. primary (worst bounded) dose metric, feasible candidates only", ""]
    if constellation["pareto_mass_vs_primary_dose"]:
        md.append("| candidate sha256 | mass | primary dose |")
        md.append("| --- | ---: | ---: |")
        for row in constellation["pareto_mass_vs_primary_dose"]:
            md.append(f"| {row['candidate_sha256']} | {row['mass']:.4g} | {row['primary_dose']:.4g} |")
    else:
        md.append("No candidate passed every requirement Core evaluated, so there is no feasible Pareto set yet.")
    md += ["", "## Best feasible candidate(s)", ""]
    if constellation["best_feasible_candidates"]:
        for row in constellation["best_feasible_candidates"]:
            md.append(f"- {row['candidate_sha256']}: primary dose {row['primary_dose']:.4g}, mass {row['mass']}")
    else:
        md.append("None: no candidate in this campaign passed every requirement Core evaluated.")
    (out / "constellation.md").write_text("\n".join(md) + "\n")
    print((out / "constellation.md").read_text())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
