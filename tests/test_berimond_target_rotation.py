import tempfile
import unittest
from pathlib import Path

from bot_berimond import (
    Control,
    aci_target_was_defeated,
    addon_binding_hint,
    claim_berimond_startup,
)


class BerimondTargetRotationTests(unittest.TestCase):
    def test_startup_claim_cancels_stale_sands_command_without_arming_run(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            control = Control(Path(directory) / "control.json")
            control.save(
                {
                    "running": True,
                    "mode": "sands",
                    "transport_only": False,
                    "pending": {"kind": "adi", "target_kind": "rbc"},
                    "last_cra": {"target": {"kingdom_id": 1}},
                    "berimond_stock": {"inventory": {"10": 266}},
                }
            )

            previous = claim_berimond_startup(control)
            claimed = control.load()

            self.assertEqual(previous["mode"], "sands")
            self.assertFalse(claimed["running"])
            self.assertEqual(claimed["mode"], "berimond")
            self.assertTrue(claimed["transport_only"])
            self.assertIsNone(claimed["pending"])
            self.assertIsNone(claimed["last_cra"])
            self.assertEqual(claimed["berimond_stock"], {"inventory": {"10": 266}})

    def test_only_observed_aci_203_marks_target_defeated(self) -> None:
        self.assertTrue(aci_target_was_defeated("berimond_aci_status_203"))
        self.assertFalse(aci_target_was_defeated("berimond_aci_status_101"))
        self.assertFalse(aci_target_was_defeated("berimond_aci_status_unknown"))

    def test_startup_session_binding_before_loaded_marker_is_valid(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            listener_log = root / "listener.log"
            listener_log.write_text(
                "[09:21:25.346] startup_bound_to_session account=ventrilo\n"
                "[09:21:25.501] rbc_proxy_listener_loaded receive_only=no\n",
                encoding="utf-8",
            )
            control = Control(root / "control.json", listener_log)
            self.assertEqual(addon_binding_hint(control), "")

    def test_loaded_without_a_session_binding_reports_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            listener_log = root / "listener.log"
            listener_log.write_text(
                "[09:21:25.501] rbc_proxy_listener_loaded receive_only=no\n",
                encoding="utf-8",
            )
            control = Control(root / "control.json", listener_log)
            self.assertIn("has not seen a login", addon_binding_hint(control))


if __name__ == "__main__":
    unittest.main()
