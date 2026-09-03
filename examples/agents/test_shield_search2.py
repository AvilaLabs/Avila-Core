import unittest

import numpy as np

import shield_search2 as ss

KNOWN_STATUSES = {"pass", "fail", "inconclusive", "not_evaluated"}


# ----------------------------------------------------------------------
# Feature construction
# ----------------------------------------------------------------------


class FeatureConstructionTests(unittest.TestCase):
    def setUp(self):
        self.materials_table = {
            "polyethylene": {"density_g_cm3": "0.94"},
            "lead": {"density_g_cm3": "11.35"},
            "iron": {"density_g_cm3": "7.87"},
        }
        self.layout = ss.FeatureLayout(sorted(self.materials_table))

    def test_feature_count_matches_names(self):
        self.assertEqual(self.layout.n_features, len(self.layout.names()))
        # n materials * 3 (thickness, first one-hot, last one-hot) + layer_count + 4 ordering scalars
        n = len(self.materials_table)
        self.assertEqual(self.layout.n_features, 3 * n + 1 + 4)

    def test_thickness_by_material_sums_repeated_layers(self):
        layers = [
            {"material": "polyethylene", "thickness_cm": "20"},
            {"material": "lead", "thickness_cm": "5"},
            {"material": "polyethylene", "thickness_cm": "15"},
        ]
        vector = self.layout.vector(layers, self.materials_table)
        self.assertEqual(vector[self.layout.thickness_feature_index("polyethylene")], 35.0)
        self.assertEqual(vector[self.layout.thickness_feature_index("lead")], 5.0)
        self.assertEqual(vector[self.layout.thickness_feature_index("iron")], 0.0)

    def test_first_and_last_one_hots(self):
        layers = [
            {"material": "iron", "thickness_cm": "10"},
            {"material": "polyethylene", "thickness_cm": "40"},
            {"material": "lead", "thickness_cm": "5"},
        ]
        names = self.layout.names()
        vector = self.layout.vector(layers, self.materials_table)
        as_dict = dict(zip(names, vector))
        self.assertEqual(as_dict["first_layer_is:iron"], 1.0)
        self.assertEqual(as_dict["first_layer_is:polyethylene"], 0.0)
        self.assertEqual(as_dict["last_layer_is:lead"], 1.0)
        self.assertEqual(as_dict["last_layer_is:iron"], 0.0)

    def test_single_layer_has_no_ordering_signal(self):
        layers = [{"material": "polyethylene", "thickness_cm": "90"}]
        names = self.layout.names()
        vector = dict(zip(names, self.layout.vector(layers, self.materials_table)))
        self.assertEqual(vector["density_order_correlation"], 0.0)
        self.assertEqual(vector["adjacent_same_material_pairs"], 0.0)
        self.assertEqual(vector["distinct_materials"], 1.0)

    def test_density_order_correlation_sign(self):
        # low density (poly) then high density (lead): increasing -> +1
        increasing = [
            {"material": "polyethylene", "thickness_cm": "10"},
            {"material": "lead", "thickness_cm": "10"},
        ]
        decreasing = [
            {"material": "lead", "thickness_cm": "10"},
            {"material": "polyethylene", "thickness_cm": "10"},
        ]
        names = self.layout.names()
        v_inc = dict(zip(names, self.layout.vector(increasing, self.materials_table)))
        v_dec = dict(zip(names, self.layout.vector(decreasing, self.materials_table)))
        self.assertAlmostEqual(v_inc["density_order_correlation"], 1.0)
        self.assertAlmostEqual(v_dec["density_order_correlation"], -1.0)

    def test_adjacent_same_material_is_counted(self):
        layers = [
            {"material": "polyethylene", "thickness_cm": "10"},
            {"material": "polyethylene", "thickness_cm": "10"},
            {"material": "lead", "thickness_cm": "5"},
        ]
        names = self.layout.names()
        vector = dict(zip(names, self.layout.vector(layers, self.materials_table)))
        self.assertEqual(vector["adjacent_same_material_pairs"], 1.0)

    def test_empty_layers_is_rejected(self):
        with self.assertRaises(ValueError):
            self.layout.vector([], self.materials_table)


# ----------------------------------------------------------------------
# Ridge fit on synthetic data
# ----------------------------------------------------------------------


class RidgeFitTests(unittest.TestCase):
    def test_ridge_recovers_a_linear_signal(self):
        rng = np.random.default_rng(0)
        X = rng.normal(size=(300, 5))
        true_coef = np.array([2.0, -1.0, 0.0, 0.5, -0.25])
        y = X @ true_coef + 0.01 * rng.normal(size=300)
        model = ss.RidgeModel(alpha=0.1).fit(X, y)
        predictions = model.predict(X)
        self.assertLess(np.abs(predictions - y).max(), 0.5)

    def test_ridge_regularizes_toward_zero_as_alpha_grows(self):
        rng = np.random.default_rng(1)
        X = rng.normal(size=(50, 3))
        y = X[:, 0] * 3.0 + 0.01 * rng.normal(size=50)
        small_alpha = ss.RidgeModel(alpha=0.01).fit(X, y)
        large_alpha = ss.RidgeModel(alpha=1000.0).fit(X, y)
        self.assertGreater(np.abs(small_alpha.coef_).sum(), np.abs(large_alpha.coef_).sum())

    def test_bootstrap_not_ready_below_min_samples(self):
        rng = np.random.default_rng(2)
        X = rng.normal(size=(3, 4))
        y = rng.normal(size=3)
        model = ss.BootstrapRidge(alpha=1.0, n_bootstrap=8, min_samples=5, seed=0).fit(X, y)
        self.assertFalse(model.ready)
        with self.assertRaises(RuntimeError):
            model.predict(X)

    def test_bootstrap_ready_and_spread_is_nonnegative(self):
        rng = np.random.default_rng(3)
        X = rng.normal(size=(40, 4))
        y = X[:, 0] * 2.0 + 0.05 * rng.normal(size=40)
        model = ss.BootstrapRidge(alpha=0.5, n_bootstrap=20, min_samples=5, seed=0).fit(X, y)
        self.assertTrue(model.ready)
        mean, std = model.predict(X[:5])
        self.assertEqual(mean.shape, (5,))
        self.assertTrue(np.all(std >= 0.0))

    def test_bootstrap_spread_shrinks_with_more_data(self):
        rng = np.random.default_rng(4)

        def make(n):
            X = rng.normal(size=(n, 3))
            y = X[:, 0] * 1.5 + 0.3 * rng.normal(size=n)
            return ss.BootstrapRidge(alpha=1.0, n_bootstrap=30, min_samples=5, seed=0).fit(X, y)

        small = make(6)
        large = make(300)
        probe = np.zeros((1, 3))
        _mean_small, std_small = small.predict(probe)
        _mean_large, std_large = large.predict(probe)
        self.assertLess(std_large[0], std_small[0])

    def test_infer_direction_upper_bound(self):
        observations = [(10.0, 3.0, 7.0), (10.0, 12.0, -2.0), (10.0, 9.5, 0.5)]
        self.assertEqual(ss.infer_direction(observations), 1)

    def test_infer_direction_lower_bound(self):
        observations = [(5.0, 8.0, 3.0), (5.0, 2.0, -3.0)]
        self.assertEqual(ss.infer_direction(observations), -1)

    def test_infer_direction_unknown_without_data(self):
        self.assertIsNone(ss.infer_direction([]))
        self.assertIsNone(ss.infer_direction([(None, None, None)]))


# ----------------------------------------------------------------------
# Acquisition ordering (SurrogateBank + score_pool), including the
# sibling/residual correction ("the transport model corrects the screen
# model where transport data exist").
# ----------------------------------------------------------------------


def fake_screen_transport_report(thickness_poly, transport, scale=3.0):
    """A synthetic Core report shaped like a real one: R1 (screen, nominal)
    and R2 (transport, bounded) share output_slot "dose-rate"; R2's true
    nominal is `scale` times R1's, mimicking CASE-001's screen being
    optimistic. R3/R4 are simple linear geometry requirements."""
    import math

    screen_nominal = 100.0 * math.exp(-0.05 * thickness_poly)
    margins = [
        {
            "requirement_id": "SHIELD-R1-screen",
            "status": "pass" if screen_nominal <= 10 else "fail",
            "rule": "nominal.le.within" if screen_nominal <= 10 else "nominal.le.exceeds",
            "unit": "uSv/h", "limit": "10", "nominal": repr(screen_nominal),
            "margin": repr(10 - screen_nominal),
        },
        {
            "requirement_id": "SHIELD-R3-mass", "status": "pass", "rule": "bounded.le.within",
            "unit": "kg", "limit": "1500", "nominal": "800", "margin": "700",
        },
        {
            "requirement_id": "SHIELD-R4-thickness", "status": "pass", "rule": "bounded.le.within",
            "unit": "cm", "limit": "100", "nominal": str(thickness_poly),
            "margin": str(100 - thickness_poly),
        },
    ]
    verdicts = [
        {"requirement_id": "SHIELD-R1-screen", "metric": {"step_id": "screen", "output_slot": "dose-rate"}},
        {"requirement_id": "SHIELD-R3-mass", "metric": {"step_id": "screen", "output_slot": "mass"}},
        {"requirement_id": "SHIELD-R4-thickness", "metric": {"step_id": "screen", "output_slot": "thickness"}},
        {"requirement_id": "SHIELD-R2-transport", "metric": {"step_id": "transport", "output_slot": "dose-rate"}},
    ]
    if transport:
        transport_nominal = scale * screen_nominal
        margins.append({
            "requirement_id": "SHIELD-R2-transport",
            "status": "pass" if transport_nominal <= 10 else "fail",
            "rule": "bounded.le.within" if transport_nominal <= 10 else "bounded.le.exceeds",
            "unit": "uSv/h", "limit": "10", "nominal": repr(transport_nominal),
            "margin": repr(10 - transport_nominal),
        })
    else:
        margins.append({"requirement_id": "SHIELD-R2-transport", "status": "not_evaluated", "rule": "not_evaluated.missing"})
    # A real --json report's compiled contract states every requirement's
    # limit, unit, and comparison unconditionally, before anything has been
    # evaluated -- this is what lets a requirement's margin be computed even
    # with zero observations of its own (see CampaignDataset.requirement_specs).
    compiled_requirements = [
        {"requirement_id": "SHIELD-R1-screen", "comparison": "less_than_or_equal",
         "limit": {"kind": "nuclear.ambient-dose-equivalent-rate", "value": "10", "unit": "uSv/h"},
         "metric": {"source": "step_output", "step_id": "screen", "output_slot": "dose-rate"}, "basis": {"kind": "nominal"}},
        {"requirement_id": "SHIELD-R2-transport", "comparison": "less_than_or_equal",
         "limit": {"kind": "nuclear.ambient-dose-equivalent-rate", "value": "10", "unit": "uSv/h"},
         "metric": {"source": "step_output", "step_id": "transport", "output_slot": "dose-rate"}, "basis": {"kind": "bounded"}},
        {"requirement_id": "SHIELD-R3-mass", "comparison": "less_than_or_equal",
         "limit": {"kind": "core.mass", "value": "1500", "unit": "kg"},
         "metric": {"source": "step_output", "step_id": "screen", "output_slot": "mass"}, "basis": {"kind": "bounded"}},
        {"requirement_id": "SHIELD-R4-thickness", "comparison": "less_than_or_equal",
         "limit": {"kind": "core.length", "value": "100", "unit": "cm"},
         "metric": {"source": "step_output", "step_id": "screen", "output_slot": "thickness"}, "basis": {"kind": "bounded"}},
    ]
    return {
        "status": "evaluated",
        "margins": margins,
        "campaign": {"verdicts": verdicts},
        "compile": {"compiled": {"requirements": compiled_requirements}},
    }


class AcquisitionOrderingTests(unittest.TestCase):
    def setUp(self):
        self.materials_table = {"polyethylene": {"density_g_cm3": "0.94"}, "lead": {"density_g_cm3": "11.35"}}
        self.layout = ss.FeatureLayout(sorted(self.materials_table))
        self.dataset = ss.CampaignDataset(self.layout, self.materials_table)
        for i, t in enumerate(range(40, 100, 3)):
            candidate = {"candidate_id": f"c-{i}", "layers": [{"material": "polyethylene", "thickness_cm": str(t)}]}
            self.dataset.add_report(candidate, fake_screen_transport_report(t, transport=False))
        # Deliberately below min_samples (5): R2 must still lack its own
        # model and fall back to the sibling correction from R1.
        for i, t in enumerate([50, 70]):
            candidate = {"candidate_id": f"t-{i}", "layers": [{"material": "polyethylene", "thickness_cm": str(t)}]}
            self.dataset.add_report(candidate, fake_screen_transport_report(t, transport=True))
        self.bank = ss.SurrogateBank(self.dataset, alpha=1.0, n_bootstrap=12, min_samples=5, residual_min_samples=2, seed=0)
        self.bank.refit()

    def test_own_model_ready_for_heavily_observed_requirement(self):
        self.assertTrue(self.bank.own_models["SHIELD-R1-screen"].ready)

    def test_sparse_requirement_falls_back_to_sibling_correction(self):
        prediction = self.bank.predict_margin("SHIELD-R2-transport", np.stack([
            self.layout.vector([{"material": "polyethylene", "thickness_cm": "60"}], self.materials_table)
        ]))
        self.assertIsNotNone(prediction)
        _mean, _std, source = prediction
        self.assertIn(source, ("corrected", "prior"))

    def test_corrected_prediction_tracks_the_scaled_truth(self):
        import math
        thickness = 60.0
        features = np.stack([
            self.layout.vector([{"material": "polyethylene", "thickness_cm": str(thickness)}], self.materials_table)
        ])
        mean_margin, _std, source = self.bank.predict_margin("SHIELD-R2-transport", features)
        self.assertEqual(source, "corrected")
        true_transport_nominal = 3.0 * 100.0 * math.exp(-0.05 * thickness)
        true_margin = 10 - true_transport_nominal
        self.assertLess(abs(mean_margin[0] - true_margin), 3.0)

    def test_thicker_slab_scores_better_than_a_thin_one(self):
        thin = np.stack([self.layout.vector([{"material": "polyethylene", "thickness_cm": "40"}], self.materials_table)])
        thick = np.stack([self.layout.vector([{"material": "polyethylene", "thickness_cm": "85"}], self.materials_table)])
        both = np.concatenate([thin, thick])
        requirement_ids = sorted(self.dataset.requirement_ids())
        scored = ss.score_pool(self.bank, requirement_ids, both, beta=0.0)
        self.assertIsNotNone(scored)
        score, _worst_mean, _binding, _used = scored
        self.assertGreater(score[1], score[0])

    def test_exploration_bonus_favors_the_less_certain_candidate(self):
        # Two candidates with identical predicted mean margin via a rigged
        # requirement: only spread should break the tie.
        requirement_ids = ["only"]

        class StubBank:
            def predict_margin(self, requirement_id, X):
                n = X.shape[0]
                # first row: confident; second row: same mean, wide spread
                return np.array([1.0, 1.0])[:n], np.array([0.0, 5.0])[:n], "own"

        features = np.zeros((2, 1))
        score, worst_mean, _binding, _used = ss.score_pool(StubBank(), requirement_ids, features, beta=1.0)
        np.testing.assert_allclose(worst_mean, [1.0, 1.0])
        self.assertGreater(score[1], score[0])

    def test_score_pool_returns_none_with_no_model_at_all(self):
        empty_bank = ss.SurrogateBank(ss.CampaignDataset(self.layout, self.materials_table))
        empty_bank.refit()
        features = np.stack([self.layout.vector([{"material": "polyethylene", "thickness_cm": "40"}], self.materials_table)])
        self.assertIsNone(ss.score_pool(empty_bank, ["SHIELD-R1-screen"], features, beta=1.0))

    def test_screened_candidates_use_the_observed_margin_not_a_prediction(self):
        # SHIELD-R3-mass is deterministic and has already been measured
        # exactly by the screen for every candidate in setUp; finalist
        # ranking must use that real, zero-uncertainty number, not a
        # regression guess with its own (nonzero) bootstrap spread.
        requirement_ids = sorted(self.dataset.requirement_ids())
        candidate_ids = list(self.dataset.records)
        scored = ss.score_screened_candidates(self.bank, self.dataset, requirement_ids, candidate_ids, beta=1.0)
        self.assertIsNotNone(scored)
        score, worst_mean, binding = scored
        for candidate_id, mean, bound in zip(candidate_ids, worst_mean, binding):
            record = self.dataset.records[candidate_id]
            observed = {rid: d for rid, d in record["requirements"].items() if d.get("status") != "not_evaluated"}
            if bound in observed and observed[bound].get("margin") is not None:
                self.assertAlmostEqual(mean, observed[bound]["margin"], places=6)

    def test_screened_candidate_falls_back_to_prediction_for_unevaluated_requirement(self):
        # A candidate that was only ever screened (never transported) has
        # no real SHIELD-R2-transport margin; scoring it must still be able
        # to use the sibling-corrected prediction for that one requirement.
        requirement_ids = ["SHIELD-R2-transport"]
        candidate_ids = ["c-0"]  # thickness 40, screen-only in setUp
        scored = ss.score_screened_candidates(self.bank, self.dataset, requirement_ids, candidate_ids, beta=1.0)
        self.assertIsNotNone(scored)
        _score, worst_mean, binding = scored
        self.assertEqual(binding[0], "SHIELD-R2-transport")
        self.assertIsNotNone(worst_mean[0])

    def test_sensitivity_sign_matches_attenuation(self):
        sensitivity = self.bank.sensitivity_to_thickness("SHIELD-R1-screen", self.layout)
        self.assertLess(sensitivity["polyethylene"], 0.0)

    def test_sibling_prior_activates_before_any_transport_data_exists(self):
        # A screen-only campaign: SHIELD-R2-transport has ZERO observations
        # of its own (not even one), which is exactly the state it is in
        # before the very first transport call. Choosing that first call
        # is precisely when a prior from the sibling (R1) matters most, so
        # it must not require R2 to already have data to bootstrap from.
        materials_table = {"polyethylene": {"density_g_cm3": "0.94"}}
        layout = ss.FeatureLayout(["polyethylene"])
        dataset = ss.CampaignDataset(layout, materials_table)
        for i, t in enumerate(range(40, 100, 3)):
            candidate = {"candidate_id": f"c-{i}", "layers": [{"material": "polyethylene", "thickness_cm": str(t)}]}
            dataset.add_report(candidate, fake_screen_transport_report(t, transport=False))
        bank = ss.SurrogateBank(dataset, alpha=1.0, n_bootstrap=12, min_samples=5, residual_min_samples=2, seed=0)
        bank.refit()
        self.assertTrue(bank.own_models["SHIELD-R1-screen"].ready)
        own_r2 = bank.own_models.get("SHIELD-R2-transport")
        self.assertFalse(own_r2 is not None and own_r2.ready)

        features = np.stack([
            layout.vector([{"material": "polyethylene", "thickness_cm": "60"}], materials_table)
        ])
        prediction = bank.predict_margin("SHIELD-R2-transport", features)
        self.assertIsNotNone(prediction, "a sibling-only prior must be available with zero of R2's own data")
        _mean, std, source = prediction
        self.assertEqual(source, "prior")
        self.assertGreater(std[0], 0.0)

        # sensitivity must not crash on a pure (no-residual) sibling prior either
        sensitivity = bank.sensitivity_to_thickness("SHIELD-R2-transport", layout)
        self.assertIsNotNone(sensitivity)


# ----------------------------------------------------------------------
# Stopping rule
# ----------------------------------------------------------------------


class StoppingRuleTests(unittest.TestCase):
    def test_first_observation_is_an_improvement(self):
        best, rounds, improved = ss.stopping_decision(None, 0, 4.0)
        self.assertEqual(best, 4.0)
        self.assertEqual(rounds, 0)
        self.assertTrue(improved)

    def test_strictly_better_resets_patience(self):
        best, rounds, improved = ss.stopping_decision(2.0, 3, 5.0)
        self.assertEqual(best, 5.0)
        self.assertEqual(rounds, 0)
        self.assertTrue(improved)

    def test_worse_or_equal_increments_patience(self):
        best, rounds, improved = ss.stopping_decision(5.0, 2, 5.0)
        self.assertEqual(best, 5.0)
        self.assertEqual(rounds, 3)
        self.assertFalse(improved)

        best2, rounds2, improved2 = ss.stopping_decision(5.0, 2, 1.0)
        self.assertEqual(best2, 5.0)
        self.assertEqual(rounds2, 3)
        self.assertFalse(improved2)

    def test_no_evaluation_this_round_still_increments_patience(self):
        best, rounds, improved = ss.stopping_decision(5.0, 1, None)
        self.assertEqual(best, 5.0)
        self.assertEqual(rounds, 2)
        self.assertFalse(improved)

    def test_budget_and_patience_loop_condition(self):
        # Mirrors main()'s while-condition: exercise both branches of the OR.
        def keep_going(screen_calls, screen_budget, rounds_since_improvement, patience):
            return screen_calls < screen_budget and rounds_since_improvement < patience

        self.assertFalse(keep_going(200, 200, 0, 5))  # budget exhausted
        self.assertFalse(keep_going(10, 200, 5, 5))  # patience exhausted
        self.assertTrue(keep_going(10, 200, 4, 5))


# ----------------------------------------------------------------------
# Constellation summary on a synthetic log
# ----------------------------------------------------------------------


def log_row(candidate_sha, **requirements):
    """`requirements`: requirement_id -> (status, rule, unit, nominal_or_None, margin_or_None)."""
    verdicts = []
    for requirement_id, (status, rule, unit, nominal, margin) in requirements.items():
        entry = {"requirement_id": requirement_id, "status": status, "rule": rule, "unit": unit}
        if nominal is not None:
            entry["nominal"] = nominal
        if margin is not None:
            entry["margin"] = margin
        verdicts.append(entry)
    return {"supplied_inputs": [{"input_id": "candidate", "sha256": candidate_sha}], "verdicts": verdicts}


class ConstellationTests(unittest.TestCase):
    def setUp(self):
        self.rows = [
            log_row(
                "sha256:aaa",
                **{
                    "SHIELD-R1-screen": ("pass", "nominal.le.within", "uSv/h", "3", "7"),
                    "SHIELD-R2-neutron": ("pass", "bounded.le.within", "uSv/h", "6", "4"),
                    "SHIELD-R3-photon": ("pass", "bounded.le.within", "uSv/h", "2", "8"),
                    "SHIELD-R4-mass": ("pass", "bounded.le.within", "kg", "900", "600"),
                    "SHIELD-R5-thickness": ("pass", "bounded.le.within", "cm", "80", "20"),
                    "SHIELD-R6-activation": ("pass", "nominal.le.within", "Bq/g", "50", "450"),
                },
            ),
            log_row(
                "sha256:bbb",
                **{
                    "SHIELD-R1-screen": ("pass", "nominal.le.within", "uSv/h", "5", "5"),
                    "SHIELD-R2-neutron": ("fail", "bounded.le.exceeds", "uSv/h", "12", "-2"),
                    "SHIELD-R3-photon": ("pass", "bounded.le.within", "uSv/h", "3", "7"),
                    "SHIELD-R4-mass": ("pass", "bounded.le.within", "kg", "1000", "500"),
                    "SHIELD-R5-thickness": ("pass", "bounded.le.within", "cm", "90", "10"),
                    "SHIELD-R6-activation": ("pass", "nominal.le.within", "Bq/g", "80", "420"),
                },
            ),
            log_row(
                "sha256:ccc",
                **{
                    "SHIELD-R1-screen": ("pass", "nominal.le.within", "uSv/h", "4", "6"),
                    "SHIELD-R2-neutron": ("inconclusive", "bounded.le.crossing", "uSv/h", None, None),
                    "SHIELD-R3-photon": ("not_evaluated", "not_evaluated.outside_qualification", "uSv/h", None, None),
                    "SHIELD-R4-mass": ("pass", "bounded.le.within", "kg", "700", "800"),
                    "SHIELD-R5-thickness": ("pass", "bounded.le.within", "cm", "110", "-10"),
                    "SHIELD-R6-activation": ("pass", "nominal.le.within", "Bq/g", "60", "440"),
                },
            ),
            log_row(
                "sha256:ddd",
                **{
                    "SHIELD-R1-screen": ("pass", "nominal.le.within", "uSv/h", "3.5", "6.5"),
                    "SHIELD-R2-neutron": ("pass", "bounded.le.within", "uSv/h", "8", "2"),
                    "SHIELD-R3-photon": ("pass", "bounded.le.within", "uSv/h", "4", "6"),
                    "SHIELD-R4-mass": ("pass", "bounded.le.within", "kg", "500", "1000"),
                    "SHIELD-R5-thickness": ("pass", "bounded.le.within", "cm", "50", "50"),
                    "SHIELD-R6-activation": ("pass", "nominal.le.within", "Bq/g", "40", "460"),
                },
            ),
        ]

    def _bank(self):
        materials_table = {"polyethylene": {"density_g_cm3": "0.94"}}
        layout = ss.FeatureLayout(["polyethylene"])
        dataset = ss.CampaignDataset(layout, materials_table)
        for i, row in enumerate(self.rows):
            candidate = {"candidate_id": f"c{i}", "layers": [{"material": "polyethylene", "thickness_cm": str(30 + i)}]}
            margins = [
                {k: v for k, v in entry.items() if k != "candidate_sha"}
                for entry in row["verdicts"]
            ]
            dataset.add_report(candidate, {"margins": margins, "campaign": {"verdicts": []}})
        bank = ss.SurrogateBank(dataset, min_samples=100)
        bank.refit()
        return bank, layout

    def test_status_counts_are_exact(self):
        constellation = ss.build_constellation(self.rows)
        req = constellation["requirements"]
        self.assertEqual(req["SHIELD-R2-neutron"]["pass"], 2)
        self.assertEqual(req["SHIELD-R2-neutron"]["fail"], 1)
        self.assertEqual(req["SHIELD-R2-neutron"]["inconclusive"], 1)
        self.assertEqual(req["SHIELD-R2-neutron"]["not_evaluated"], 0)
        self.assertEqual(req["SHIELD-R3-photon"]["not_evaluated"], 1)

    def test_not_evaluated_and_inconclusive_are_distinct_from_pass_fail(self):
        constellation = ss.build_constellation(self.rows)
        photon = constellation["requirements"]["SHIELD-R3-photon"]
        neutron = constellation["requirements"]["SHIELD-R2-neutron"]
        # not_evaluated never counts toward binding (no margin to compare)
        self.assertEqual(photon["not_evaluated"], 1)
        self.assertEqual(photon["binding"], 0)
        # inconclusive with no margin also cannot be binding
        self.assertEqual(neutron["inconclusive"], 1)

    def test_binding_constraint_tally_matches_hand_computed_minimum(self):
        constellation = ss.build_constellation(self.rows)
        req = constellation["requirements"]
        # row aaa: min margin is R2 (4); bbb: R2 (-2); ccc: R5 (-10, since R2/R3 have no margin); ddd: R2 (2)
        self.assertEqual(req["SHIELD-R2-neutron"]["binding"], 3)
        self.assertEqual(req["SHIELD-R5-thickness"]["binding"], 1)

    def test_dose_and_mass_requirement_ids_are_found_without_hard_coding(self):
        bank, _layout = self._bank()
        self.assertEqual(set(bank.dose_requirement_ids()), {"SHIELD-R2-neutron", "SHIELD-R3-photon"})
        self.assertEqual(bank.mass_requirement_ids(), ["SHIELD-R4-mass"])
        # the nominal-basis screen and the nominal-basis activation
        # requirement must NOT be swept into "primary dose metric"
        self.assertNotIn("SHIELD-R1-screen", bank.dose_requirement_ids())
        self.assertNotIn("SHIELD-R6-activation", bank.dose_requirement_ids())

    def test_pareto_front_keeps_only_non_dominated_feasible_candidates(self):
        bank, layout = self._bank()
        constellation = ss.build_constellation(self.rows, bank=bank, layout=layout)
        pareto_ids = {row["candidate_sha256"] for row in constellation["pareto_mass_vs_primary_dose"]}
        # aaa (mass 900, dose 6) and ddd (mass 500, dose 8): neither dominates
        # the other, so both belong on the front; bbb and ccc are infeasible.
        self.assertEqual(pareto_ids, {"sha256:aaa", "sha256:ddd"})

    def test_best_feasible_candidate_has_the_lowest_primary_dose(self):
        bank, layout = self._bank()
        constellation = ss.build_constellation(self.rows, bank=bank, layout=layout)
        self.assertEqual(constellation["best_feasible_candidates"][0]["candidate_sha256"], "sha256:aaa")

    def test_empty_log_produces_a_well_formed_empty_constellation(self):
        constellation = ss.build_constellation([])
        self.assertEqual(constellation["requirements"], {})
        self.assertEqual(constellation["pareto_mass_vs_primary_dose"], [])
        self.assertEqual(constellation["best_feasible_candidates"], [])


# ----------------------------------------------------------------------
# The designer never invents a verdict/status field; it only ever copies
# Core's own.
# ----------------------------------------------------------------------


class NoInventedVerdictTests(unittest.TestCase):
    def _walk(self, obj, path, violations):
        if isinstance(obj, dict):
            for key, value in obj.items():
                if key == "status" and isinstance(value, str):
                    if value not in KNOWN_STATUSES:
                        violations.append(f"{path}.{key} = {value!r}")
                if key == "verdict":
                    violations.append(f"{path}.{key} is a bare verdict object")
                self._walk(value, f"{path}.{key}", violations)
        elif isinstance(obj, list):
            for index, item in enumerate(obj):
                self._walk(item, f"{path}[{index}]", violations)

    def test_constellation_output_contains_no_invented_status(self):
        rows = ConstellationTests()
        rows.setUp()
        bank, layout = rows._bank()
        constellation = ss.build_constellation(rows.rows, bank=bank, layout=layout)
        violations = []
        self._walk(constellation, "constellation", violations)
        self.assertEqual(violations, [], f"invented verdict-shaped fields: {violations}")

    def test_every_status_value_traces_back_to_a_log_row(self):
        rows = ConstellationTests()
        rows.setUp()
        recorded = set()
        for row in rows.rows:
            for entry in row["verdicts"]:
                recorded.add((entry["requirement_id"], entry["status"]))
        # Every (requirement, status) pair with a non-zero count in the
        # constellation must have been recorded in the source log at least
        # once; the constellation cannot manufacture a status Core never
        # reported for that requirement.
        constellation = ss.build_constellation(rows.rows)
        for requirement_id, stats in constellation["requirements"].items():
            for status in KNOWN_STATUSES:
                if stats[status] > 0:
                    self.assertIn(
                        (requirement_id, status), recorded,
                        f"{requirement_id}={status} was reported but never appears in the source log",
                    )


if __name__ == "__main__":
    unittest.main()
