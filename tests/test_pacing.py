import unittest

from bot.pacing import AttackPacingPolicy, state_last_cra_at


class AttackPacingPolicyTests(unittest.TestCase):
    def setUp(self):
        self.policy = AttackPacingPolicy()

    def test_sands_adi_delay_is_sampled_without_old_twenty_second_pause(self):
        samples = iter((6.25, 0.40))
        due = self.policy.sands_cra_due_at(
            now=100.0,
            last_cra_at=98.0,
            uniform=lambda _low, _high: next(samples),
        )
        self.assertEqual(due, 106.25)

    def test_positive_jitter_extends_four_second_cra_floor(self):
        samples = iter((5.5, 0.4))
        due = self.policy.sands_cra_due_at(
            now=100.0,
            last_cra_at=102.0,
            uniform=lambda _low, _high: next(samples),
        )
        self.assertEqual(due, 106.4)

    def test_final_guard_never_allows_less_than_four_seconds(self):
        self.assertEqual(self.policy.enforce_cra_deadline(103.0, 100.0), 104.0)
        self.assertEqual(self.policy.enforce_cra_deadline(108.0, 100.0), 108.0)

    def test_newest_timestamp_wins_over_legacy_fallback(self):
        state = {
            "last_cra_injected_at": 100.75,
            "last_global_attack_sent_at": 100,
        }
        self.assertEqual(state_last_cra_at(state), 100.75)
        self.assertEqual(
            state_last_cra_at(
                {"last_cra_injected_at": 100.75, "last_global_attack_sent_at": 101.25}
            ),
            101.25,
        )
        self.assertEqual(state_last_cra_at({"last_global_attack_sent_at": 99}), 99.0)
        self.assertIsNone(state_last_cra_at({}))

    def test_normal_success_delay_remains_variable(self):
        self.assertEqual(
            self.policy.sands_after_cra_ack_delay(uniform=lambda low, high: (low + high) / 2),
            1.5,
        )


if __name__ == "__main__":
    unittest.main()
