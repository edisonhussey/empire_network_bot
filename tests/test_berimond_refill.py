import unittest

from bot.event.berimond_kingdom import config
from bot.event.berimond_kingdom.refill import (
    headquarters_occupancy,
    plan_refill,
    projected_unit_counts,
    survival_ratios,
)


class BerimondRefillTests(unittest.TestCase):
    def test_refill_targets_configured_safety_fraction(self):
        target = int(config.TOTAL_CAPACITY * 0.7)
        self.assertEqual(plan_refill(sent={10: 14}, expected={10: 0}, safety=0.7), [[10, target]])
        self.assertEqual(plan_refill(sent={10: 14}, expected={10: 202}, safety=0.7), [[10, target - 202]])
        self.assertEqual(plan_refill(sent={10: 14}, expected={10: target}, safety=0.7), [])

    def test_projected_count_uses_exact_returns_and_measured_survival(self):
        state = {
            "berimond_stock": {
                "inventory": {"10": 9},
                "transit": {"10": 139},
                "at": 100,
            },
            "berimond_returns": [
                {"arrives_at": 130, "units": {"10": 11, "14": 9}},
                {"arrives_at": 90, "units": {"10": 9, "14": 8}},
            ],
        }
        ratios = survival_ratios(state, {10: 14, 14: 12})
        self.assertAlmostEqual(ratios[10], 20 / 28)
        self.assertAlmostEqual(ratios[14], 17 / 24)
        projection = projected_unit_counts(state, {10: 14, 14: 12}, now=110)[10]
        self.assertEqual(projection["home"], 9)
        self.assertEqual(projection["assigned"], 139)
        self.assertEqual(projection["returning"], 11)
        self.assertAlmostEqual(projection["total"], 9 + 139 * (20 / 28) + 11)

    def test_raw_home_plus_assigned_is_available_for_diagnostics(self):
        state = {
            "berimond_stock": {
                "inventory": {"10": 9},
                "transit": {"10": 193},
            }
        }
        occupancy = headquarters_occupancy(state)
        self.assertEqual(occupancy, {10: 202})

    def test_no_return_history_assumes_all_assigned_troops_survive(self):
        state = {
            "berimond_stock": {
                "inventory": {"10": 9},
                "transit": {"10": 193},
                "at": 100,
            }
        }
        projection = projected_unit_counts(state, {10: 14}, now=110)[10]
        self.assertEqual(projection["survival"], 1.0)
        self.assertEqual(projection["total"], 202)
        target = int(config.TOTAL_CAPACITY * 0.7)
        self.assertEqual(plan_refill(sent={10: 14}, expected={10: 202}, safety=0.7), [[10, target - 202]])

    def test_unknown_unit_is_not_refilled(self):
        self.assertEqual(plan_refill(sent={999: 10}, expected={999: 0}), [])


if __name__ == "__main__":
    unittest.main()
