import unittest

from bot.proxy_transport import (
    berimond_attack_config,
    command_allowed,
    command_mode,
    record_sands_adi_timeout_error,
    response_matches,
    sands_adi_timeout_age,
)


class ProxyTransportTests(unittest.TestCase):
    def test_sands_adi_timeout_allows_two_errors_then_stops(self):
        state = {"cra_consecutive_errors": 0, "cra_error_timestamps": []}

        first = record_sands_adi_timeout_error(state, now=100, tolerated_errors=2)
        second = record_sands_adi_timeout_error(state, now=101, tolerated_errors=2)
        third = record_sands_adi_timeout_error(state, now=102, tolerated_errors=2)

        self.assertEqual(first, (1, [100], False))
        self.assertEqual(second, (2, [100, 101], False))
        self.assertEqual(third, (3, [100, 101, 102], True))

    def test_sands_adi_timeout_starts_only_after_deadline(self):
        pending = {"kind": "adi", "sent_at": 100.0}
        timeout = 45.0

        self.assertIsNone(
            sands_adi_timeout_age(
                pending,
                timeout_seconds=timeout,
                now=100.0 + timeout,
            )
        )
        self.assertEqual(
            sands_adi_timeout_age(
                pending,
                timeout_seconds=timeout,
                now=101.0 + timeout,
            ),
            timeout + 1.0,
        )

    def test_sands_pending_is_rejected_in_berimond_mode(self):
        state = {"running": True, "mode": "berimond"}
        pending = {"kind": "cra", "target_kind": "rbc"}
        self.assertEqual(command_mode(pending), "sands")
        self.assertFalse(command_allowed(state, pending))

    def test_berimond_commands_require_live_berimond_owner(self):
        command = {"kind": "kut_transfer", "command_id": "refill-1"}
        self.assertTrue(command_allowed({"running": True, "mode": "berimond"}, command))
        self.assertFalse(command_allowed({"running": False, "mode": "berimond"}, command))

    def test_time_skip_requires_live_berimond_owner(self):
        command = {"kind": "msk_skip", "mst": "MS5", "kid": 10, "tt": 1}
        self.assertTrue(command_allowed({"running": True, "mode": "berimond"}, command))
        self.assertFalse(command_allowed({"running": True, "mode": "sands"}, command))
        self.assertFalse(command_allowed({"running": False, "mode": "berimond"}, command))
        self.assertFalse(command_allowed({"running": True, "mode": None}, command))

    def test_refill_response_must_match_command_id(self):
        command = {"kind": "kut_transfer", "command_id": "refill-1"}
        self.assertTrue(response_matches(command, {"command_id": "refill-1", "status": 0}))
        self.assertFalse(response_matches(command, {"command_id": "older-refill", "status": 0}))
        self.assertFalse(response_matches(command, None))

    def test_command_army_overrides_long_running_proxy_state(self):
        old_army = [{"L": {"U": [[10, 14]]}}]
        edited_army = [{"L": {"U": [[10, 22], [14, 4]]}}]
        state = {"berimond_attack": old_army}
        command = {"kind": "aci", "berimond_attack": edited_army}
        self.assertIs(berimond_attack_config(command, state), edited_army)

    def test_run_army_is_fallback_for_legacy_command(self):
        run_army = [{"L": {"U": [[10, 22]]}}]
        self.assertIs(berimond_attack_config({"kind": "aci"}, {"berimond_attack": run_army}), run_army)

    def test_unknown_or_unowned_command_fails_closed(self):
        state = {"running": True, "mode": "berimond"}
        self.assertFalse(command_allowed(state, {"kind": "mystery"}))
        self.assertFalse(command_allowed({"running": True, "mode": None}, {"kind": "cra"}))


if __name__ == "__main__":
    unittest.main()
