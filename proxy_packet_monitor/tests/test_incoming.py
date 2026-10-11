from __future__ import annotations
import json
import tempfile
import unittest
from pathlib import Path

from backend.incoming import IncomingLedger
from backend.protocol import parse_xt_packet

SAMPLE = (Path(__file__).parent / "fixtures" / "gam_sample.txt").read_text(encoding="utf-8").strip()


def packet(payload):
    return "%xt%gam%1%0%" + json.dumps(payload, separators=(",", ":")) + "%"


class IncomingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.ledger = IncomingLedger(Path(self.temporary.name) / "location.json")
        self.ledger.configure({"own_kid": 0, "own_x": 677, "own_y": 688})

    def tearDown(self):
        self.temporary.cleanup()

    def test_sample_fields_and_arrival(self):
        added = self.ledger.observe(SAMPLE, 1000)
        self.assertEqual(len(added), 1)
        item = added[0]
        self.assertEqual((item["mid"], item["pt"], item["tt"], item["kid"]), (155072784, 464, 557, 0))
        self.assertEqual(item["arrival_at"], 1093)
        self.assertEqual(item["source"], {"x": 647, "y": 759})
        self.assertEqual(item["target"], {"x": 677, "y": 688})
        self.assertEqual(item["sender"]["name"], "Great Emperor 7")
        self.assertEqual(item["receiver"]["name"], "Great Emperor 8")
        self.assertEqual(item["composition"]["troops"], None)
        self.assertEqual(self.ledger.snapshot(1001)["active_count"], 1)
        self.assertEqual(self.ledger.snapshot(1094)["active_count"], 0)
        self.assertEqual(len(self.ledger.snapshot(1094)["arrived"]), 1)

    def test_scope_and_first_observation_wins(self):
        payload = parse_xt_packet(SAMPLE)["payload"]
        self.ledger.configure({"own_kid": 0, "own_x": 1, "own_y": 2})
        self.assertEqual(self.ledger.observe(SAMPLE, 1000), [])
        self.ledger.configure({"own_kid": 1, "own_x": 677, "own_y": 688})
        self.assertEqual(self.ledger.observe(SAMPLE, 1000), [])
        self.ledger.configure({"own_kid": 0, "own_x": 677, "own_y": 688})
        self.ledger.observe(SAMPLE, 1000)
        self.assertEqual(self.ledger.observe(SAMPLE, 1020), [])
        self.assertEqual(self.ledger.snapshot(1001)["active"][0]["arrival_at"], 1093)
        self.ledger.configure({"own_kid": 0, "own_x": 700, "own_y": 800})
        self.assertEqual(self.ledger.snapshot(1001)["active_count"], 0)

    def test_multiple_movements_player_order_and_unknowns(self):
        payload = parse_xt_packet(SAMPLE)["payload"]
        second = json.loads(json.dumps(payload["M"][0]))
        second["M"]["MID"] = 155072785
        second["M"]["PT"] = 500
        payload["M"].append(second)
        payload["O"].reverse()
        self.assertEqual(len(self.ledger.observe(packet(payload), 1000)), 2)
        self.assertEqual(self.ledger.snapshot(1001)["active"][0]["sender"]["name"], "Great Emperor 7")
        payload["M"][0]["M"]["MID"] = 155072786
        payload["M"][1]["M"]["MID"] = 155072787
        payload["O"] = []
        added = self.ledger.observe(packet(payload), 1000)
        self.assertEqual(len(added), 2)
        self.assertIsNone(added[0]["sender"]["name"])
        self.assertEqual(self.ledger.observe('%xt%adi%1%0%{}%', 1000), [])
        self.assertEqual(self.ledger.observe('%xt%gam%1%0%{"M":[null,{}]}%', 1000), [])


if __name__ == "__main__":
    unittest.main()
