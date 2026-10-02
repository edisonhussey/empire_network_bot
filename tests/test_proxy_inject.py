import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from bot import proxy_inject
from bot.account_context import AccountContext
from bot.packets import parse_xt_packet


def temporary_context(root: Path) -> AccountContext:
    return AccountContext(
        username="tester",
        aid="tester",
        root=root,
        logs_dir=root / "logs",
        latest_logs_dir=root / "latest_logs",
        gamestate_dir=root / "gamestate",
        control_file=root / "proxy_control.json",
        listener_log=root / "listener.log",
    )


class ProxyInjectTests(unittest.TestCase):
    def test_build_packet_accepts_command_and_payload(self):
        packet = proxy_inject.build_packet("gaa", {"KID": 1, "AX1": 10})
        parsed = parse_xt_packet(packet)

        self.assertIsNotNone(parsed)
        self.assertEqual(parsed["command"], "gaa")
        self.assertEqual(parsed["payload"], {"KID": 1, "AX1": 10})

    def test_inject_atomically_queues_for_selected_account(self):
        with tempfile.TemporaryDirectory() as folder:
            context = temporary_context(Path(folder))
            with mock.patch.object(proxy_inject, "_context", return_value=context):
                result = proxy_inject.inject("gaa", {"KID": 1}, wait_seconds=0)

            self.assertEqual(result["status"], "queued")
            request_path = Path(result["request_path"])
            self.assertTrue(request_path.is_file())
            request = json.loads(request_path.read_text(encoding="utf-8"))
            self.assertEqual(request["account"], "tester")
            self.assertEqual(request["command"], "gaa")

    def test_claim_and_finish_publish_listener_result(self):
        with tempfile.TemporaryDirectory() as folder:
            context = temporary_context(Path(folder))
            with mock.patch.object(proxy_inject, "_context", return_value=context):
                queued = proxy_inject.inject("gaa", {"KID": 1}, wait_seconds=0)

            claimed_path, request = proxy_inject.claim_next(context)
            self.assertTrue(claimed_path.name.endswith(".processing.json"))
            proxy_inject.finish(context, claimed_path, request, status="sent", sent_at=123.0)

            result_path = proxy_inject.injection_dir(context) / f"{queued['id']}.result.json"
            result = json.loads(result_path.read_text(encoding="utf-8"))
            self.assertEqual(result["status"], "sent")
            self.assertEqual(result["sent_at"], 123.0)
            self.assertFalse(claimed_path.exists())

    def test_invalid_raw_packet_is_rejected_before_queueing(self):
        with tempfile.TemporaryDirectory() as folder:
            context = temporary_context(Path(folder))
            with mock.patch.object(proxy_inject, "_context", return_value=context):
                with self.assertRaisesRegex(ValueError, "valid XT packet"):
                    proxy_inject.inject("not a packet", wait_seconds=0)


if __name__ == "__main__":
    unittest.main()
