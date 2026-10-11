from __future__ import annotations

import json
import sqlite3
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from backend.auto_support import AutoSupport, KNOWN_TROOP_IDS, SupportNotSent, load_config, opposing_alliances, save_config, select_troops, troops_from_dcl
from backend.protocol import parse_xt_packet


ROOT = Path(__file__).resolve().parents[1]


class AutoSupportTests(unittest.TestCase):
    def test_latest_attack_targets_owned_castle_then_retreats(self):
        with tempfile.TemporaryDirectory() as folder:
            config_path = Path(folder) / "config.json"
            save_config(config_path, {"enabled": True, "castles": [{"kid": 0, "x": 543, "y": 520}],
                                      "destinations": {"0": {"x": 468, "y": 507}}, "lid": 0})
            support = AutoSupport(config_path, Path(folder) / "status.json")
            support.observe((ROOT / "tests" / "fixtures" / "gam_156095506.txt").read_text(), 1000)
            job = support.jobs[156095506]
            self.assertEqual((job["sx"], job["sy"], job["sender_alliance_id"],
                              job["receiver_alliance_id"]), (543, 520, 3652, 3026))
            self.assertEqual(job["landing"], 1181)
            support.observe('%xt%mrm%1%0%{"MID":156095506}%', 1010)
            self.assertEqual(job["state"], "cancelled")

    def test_alliance_rule_uses_player_ids_not_array_positions(self):
        captured = parse_xt_packet((ROOT / "learned_info" / "gam.txt").read_text())["payload"]
        movement = captured["M"][0]["M"]
        players = captured["O"]
        self.assertEqual(opposing_alliances(movement, list(reversed(players))), (3026, 3652))
        self.assertIsNone(opposing_alliances(movement, players[:1]))
        same = [dict(person, AID=3026) for person in players]
        self.assertIsNone(opposing_alliances(movement, same))
        unknown = [dict(person) for person in players]
        del unknown[1]["AID"]
        self.assertIsNone(opposing_alliances(movement, unknown))
        original = parse_xt_packet((ROOT / "tests" / "fixtures" / "gam_sample.txt").read_text())["payload"]
        self.assertIsNone(opposing_alliances(original["M"][0]["M"], original["O"]))

    def test_config_starts_off_and_requires_destinations(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "config.json"
            self.assertFalse(load_config(path)["enabled"])
            with self.assertRaisesRegex(ValueError, "destination"):
                save_config(path, {"enabled": True, "castles": [{"kid": 0, "x": 1, "y": 2}],
                                   "destinations": {}, "lid": 0})
            saved = save_config(path, {"enabled": True, "castles": [{"kid": 0, "x": 1, "y": 2}],
                                       "destinations": {"0": {"x": 3, "y": 4}}, "lid": 0})
            self.assertEqual(load_config(path), saved)

    def test_dcl_castle_and_greedy_home_units(self):
        raw = (ROOT / "learned_info" / "dcl.txt").read_text()
        payload = parse_xt_packet(next(line for line in raw.splitlines() if line.startswith("%xt%dcl%")))["payload"]
        stock = troops_from_dcl(payload, 0, 437391)
        self.assertEqual(stock, {602: 9, 614: 5, 660: 3})
        self.assertIsNone(troops_from_dcl(payload, 0, 999))
        chosen = select_troops({602: 9, 614: 5, 660: 3},
                               {"gui": {"I": [[602, 7], [614, 5], [660, 3], [637, 0]]}})
        self.assertEqual(chosen, [[602, 7]])  # 614 and 660 are not in the troop catalog.
        ids = sorted(KNOWN_TROOP_IDS)[:12]
        varied = {unit: 30 - index for index, unit in enumerate(ids)}
        rows = [[unit, varied[unit]] for unit in reversed(ids)]
        self.assertEqual(select_troops(varied, {"gui": {"I": rows}}), rows[-10:][::-1])

    def test_packet_sequence_and_duplicate_mid(self):
        with tempfile.TemporaryDirectory() as folder:
            config_path = Path(folder) / "config.json"
            save_config(config_path, {"enabled": True, "castles": [{"kid": 0, "x": 677, "y": 688}],
                                      "destinations": {"0": {"x": 468, "y": 507}}, "lid": 0})
            support = AutoSupport(config_path, Path(folder) / "status.json")
            sent = []
            gam_payload = parse_xt_packet((ROOT / "tests" / "fixtures" / "gam_sample.txt").read_text())["payload"]
            gam_payload["O"][0]["AID"] = 3026
            gam_payload["O"][1]["AID"] = 3652
            gam = "%xt%gam%1%0%" + json.dumps(gam_payload) + "%"
            dcl = next(line for line in (ROOT / "learned_info" / "dcl.txt").read_text().splitlines()
                       if line.startswith("%xt%dcl%"))
            with patch("backend.auto_support.time.time", return_value=1000):
                support.observe((ROOT / "tests" / "fixtures" / "gam_sample.txt").read_text(), 1000)
                self.assertEqual(len(support.jobs), 0)
                support.observe(gam, 1000)
                support.observe(gam, 1001)
                self.assertEqual(len(support.jobs), 1)
            # This GAM fixture targets a different player and area from DCL.
            # A mismatched owner must fail before SDI or CDS can be sent.
            with patch("backend.auto_support.time.time", return_value=1086):
                support.tick(lambda command, payload: sent.append((command, payload)))
                self.assertEqual(sent[0][0], "dcl")
                support.observe(dcl, 1086)
            self.assertEqual(next(iter(support.jobs.values()))["state"], "failed")
            self.assertEqual(len(sent), 1)

    def test_cds_ack_requires_matching_destination(self):
        with tempfile.TemporaryDirectory() as folder:
            support = AutoSupport(Path(folder) / "config.json", Path(folder) / "status.json")
            support.jobs[123] = {"mid": 123, "kid": 0, "sx": 543, "sy": 520,
                                 "castle_id": 437391, "landing": 5000, "state": "await_cds",
                                 "destination": {"x": 468, "y": 507}, "lid": 0}
            support.active_mid = 123
            raw = (ROOT / "learned_info" / "cds.txt").read_text()
            support.observe(raw, 1000)
            self.assertEqual(support.jobs[123]["state"], "sent")
            self.assertEqual(support.jobs[123]["sent_mid"], 156018970)

    def test_cds_timeout_holds_lid_until_late_ack(self):
        with tempfile.TemporaryDirectory() as folder:
            config_path = Path(folder) / "config.json"
            save_config(config_path, {"enabled": True, "castles": [{"kid": 0, "x": 543, "y": 520}],
                                      "destinations": {"0": {"x": 468, "y": 507}}, "lid": 0})
            support = AutoSupport(config_path, Path(folder) / "status.json")
            support.jobs[123] = {"mid": 123, "kid": 0, "sx": 543, "sy": 520,
                                 "castle_id": 437391, "landing": 5000, "state": "await_cds",
                                 "requested_at": 1000, "destination": {"x": 468, "y": 507}, "lid": 0}
            support.active_mid = 123
            with patch("backend.auto_support.time.time", return_value=1003):
                support.tick(lambda *_: self.fail("unexpected duplicate send"))
            self.assertEqual(support.jobs[123]["state"], "uncertain_cds")
            self.assertEqual(support.active_mid, 123)
            support.observe((ROOT / "learned_info" / "cds.txt").read_text(), 1004)
            self.assertEqual(support.jobs[123]["state"], "sent")

    def test_disconnect_stops_old_socket_work(self):
        with tempfile.TemporaryDirectory() as folder:
            support = AutoSupport(Path(folder) / "config.json", Path(folder) / "status.json")
            support.jobs[1] = {"mid": 1, "landing": 5000, "state": "ready_sdi"}
            support.jobs[2] = {"mid": 2, "landing": 5000, "state": "await_cds"}
            support.active_mid = 2
            support.connection_lost()
            self.assertEqual(support.jobs[1]["state"], "failed")
            self.assertEqual(support.jobs[2]["state"], "uncertain_cds")
            self.assertEqual(support.active_mid, 2)

    def test_full_dcl_sdi_cds_sequence(self):
        with tempfile.TemporaryDirectory() as folder:
            config_path = Path(folder) / "config.json"
            save_config(config_path, {"enabled": True, "castles": [{"kid": 0, "x": 543, "y": 520}],
                                      "destinations": {"0": {"x": 468, "y": 507}}, "lid": 0})
            support = AutoSupport(config_path, Path(folder) / "status.json")
            sent = []
            gam = parse_xt_packet((ROOT / "tests" / "fixtures" / "gam_sample.txt").read_text())
            movement = gam["payload"]["M"][0]["M"]
            movement["TA"][1:4] = [543, 520, 437391]
            movement["TA"][4] = 596904
            movement["TID"] = 596904
            movement["MID"] = 123
            gam["payload"]["O"][0]["AID"] = 3026
            gam["payload"]["O"][1]["OID"] = 596904
            gam["payload"]["O"][1]["AID"] = 3652
            raw_gam = "%xt%gam%1%0%" + json.dumps(gam["payload"]) + "%"
            dcl = next(line for line in (ROOT / "learned_info" / "dcl.txt").read_text().splitlines()
                       if line.startswith("%xt%dcl%"))
            sdi = next(line for line in (ROOT / "learned_info" / "sdi.md").read_text().splitlines()
                       if line.startswith("%xt%sdi%"))
            cds_payload = parse_xt_packet((ROOT / "learned_info" / "cds.txt").read_text())["payload"]
            cds_payload["A"]["A"] = [[602, 9]]
            cds = "%xt%cds%1%0%" + json.dumps(cds_payload) + "%"
            with patch("backend.auto_support.time.time", return_value=1000):
                support.observe(raw_gam, 1000)
            with patch("backend.auto_support.time.time", return_value=1086):
                support.tick(lambda command, payload: sent.append((command, payload)))
                support.observe(dcl, 1086)
            with patch("backend.auto_support.time.time", return_value=1088):
                support.tick(lambda command, payload: sent.append((command, payload)))
                support.observe(sdi, 1088)
                support.tick(lambda command, payload: sent.append((command, payload)))
                support.observe(cds, 1088)
            self.assertEqual([name for name, _ in sent], ["dcl", "sdi", "cds"])
            self.assertEqual(sent[-1][1]["A"], [[602, 9]])
            self.assertEqual(support.jobs[123]["state"], "sent")

    def test_landing_recalls_confirmed_support_once_and_tracks_return(self):
        with tempfile.TemporaryDirectory() as folder:
            support = AutoSupport(Path(folder) / "config.json", Path(folder) / "status.json")
            support.jobs[123] = {"mid": 123, "landing": 1100, "state": "sent",
                                 "sent_mid": 156018970, "lid": 0}
            sent = []
            with patch("backend.auto_support.time.time", return_value=1099):
                support.tick(lambda command, payload: sent.append((command, payload)))
            self.assertEqual(sent, [])
            with patch("backend.auto_support.time.time", return_value=1100.1):
                support.tick(lambda command, payload: sent.append((command, payload)))
                support.tick(lambda command, payload: sent.append((command, payload)))
            self.assertEqual(sent, [("mcm", {"MID": 156018970})])
            self.assertEqual(support.jobs[123]["state"], "await_mcm")
            response = (ROOT / "tests" / "fixtures" / "mcm_response.txt").read_text()
            support.observe(response, 1101)
            self.assertEqual(support.jobs[123]["state"], "recalled")
            self.assertEqual(support.jobs[123]["return_at"], 1406)

    def test_early_retreat_recalls_sent_support_before_landing(self):
        with tempfile.TemporaryDirectory() as folder:
            support = AutoSupport(Path(folder) / "config.json", Path(folder) / "status.json")
            support.jobs[123] = {"mid": 123, "landing": 5000, "state": "sent",
                                 "sent_mid": 456, "lid": 0}
            support.observe('%xt%mrm%1%0%{"MID":123}%', 1000)
            sent = []
            with patch("backend.auto_support.time.time", return_value=1000.05):
                support.tick(lambda command, payload: sent.append((command, payload)))
            self.assertEqual(sent, [("mcm", {"MID": 456})])
            self.assertEqual(support.jobs[123]["recall_reason"], "attack retreated")

    def test_retreat_during_pending_cds_waits_for_support_mid(self):
        with tempfile.TemporaryDirectory() as folder:
            support = AutoSupport(Path(folder) / "config.json", Path(folder) / "status.json")
            support.jobs[123] = {"mid": 123, "kid": 0, "castle_id": 437391,
                                 "landing": 5000, "state": "await_cds", "lid": 0,
                                 "destination": {"x": 468, "y": 507}}
            support.active_mid = 123
            support.observe('%xt%mrm%1%0%{"MID":123}%', 1000)
            sent = []
            with patch("backend.auto_support.time.time", return_value=1000.1):
                support.tick(lambda command, payload: sent.append((command, payload)))
            self.assertEqual(sent, [])
            support.observe((ROOT / "learned_info" / "cds.txt").read_text(), 1000.2)
            with patch("backend.auto_support.time.time", return_value=1000.25):
                support.tick(lambda command, payload: sent.append((command, payload)))
            self.assertEqual(sent, [("mcm", {"MID": 156018970})])

    def test_recall_timeout_and_disconnect_do_not_retry(self):
        with tempfile.TemporaryDirectory() as folder:
            support = AutoSupport(Path(folder) / "config.json", Path(folder) / "status.json")
            support.jobs[123] = {"mid": 123, "landing": 1100, "state": "sent",
                                 "sent_mid": 156018970, "lid": 0}
            sent = []
            with patch("backend.auto_support.time.time", return_value=1101):
                support.tick(lambda command, payload: sent.append((command, payload)))
            support.connection_lost()
            self.assertEqual(support.jobs[123]["state"], "uncertain_mcm")
            with patch("backend.auto_support.time.time", return_value=1110):
                support.tick(lambda command, payload: sent.append((command, payload)))
            self.assertEqual(len(sent), 1)
            response = (ROOT / "tests" / "fixtures" / "mcm_response.txt").read_text()
            support.observe(response, 1111)
            self.assertEqual(support.jobs[123]["state"], "recalled")

    def test_recall_ack_timeout_and_occupied_lid(self):
        with tempfile.TemporaryDirectory() as folder:
            config_path = Path(folder) / "config.json"
            save_config(config_path, {"enabled": True, "castles": [{"kid": 0, "x": 543, "y": 520}],
                                      "destinations": {"0": {"x": 468, "y": 507}}, "lid": 0})
            support = AutoSupport(config_path, Path(folder) / "status.json")
            support.jobs[123] = {"mid": 123, "landing": 1100, "state": "sent",
                                 "sent_mid": 456, "lid": 0}
            support.jobs[124] = {"mid": 124, "landing": 1110, "state": "waiting", "lid": 0}
            sent = []
            with patch("backend.auto_support.time.time", return_value=1103):
                support.tick(lambda command, payload: sent.append((command, payload)))
            self.assertEqual(sent, [("mcm", {"MID": 456})])
            with patch("backend.auto_support.time.time", return_value=1104):
                support.tick(lambda command, payload: sent.append((command, payload)))
            self.assertEqual(support.jobs[123]["state"], "await_mcm")
            self.assertEqual(support.jobs[124]["state"], "failed")
            with patch("backend.auto_support.time.time", return_value=1105.1):
                support.tick(lambda command, payload: sent.append((command, payload)))
            self.assertEqual(support.jobs[123]["state"], "uncertain_mcm")
            self.assertEqual(len(sent), 1)

    def test_restart_and_toggle_off_preserve_due_recall(self):
        with tempfile.TemporaryDirectory() as folder:
            config_path = Path(folder) / "config.json"
            status_path = Path(folder) / "status.json"
            save_config(config_path, {"enabled": False, "castles": [], "destinations": {}, "lid": 0})
            status_path.write_text(json.dumps({"jobs": [
                {"mid": 123, "landing": 1100, "state": "sent", "sent_mid": 456, "lid": 0}]}))
            support = AutoSupport(config_path, status_path)
            sent = []
            with patch("backend.auto_support.time.time", return_value=1101):
                support.tick(lambda command, payload: sent.append((command, payload)))
            self.assertEqual(sent, [("mcm", {"MID": 456})])
            self.assertEqual(support.jobs[123]["state"], "await_mcm")

    def test_recorded_manual_recall_reconciles_stale_uncertain_state(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sessions = root / "sessions"
            sessions.mkdir()
            response = (ROOT / "tests" / "fixtures" / "mcm_response.txt").read_text()
            with sqlite3.connect(sessions / "recorded.sqlite3") as conn:
                conn.execute("CREATE TABLE events (received_at REAL, raw TEXT, direction TEXT, type TEXT)")
                conn.execute("INSERT INTO events VALUES (?, ?, ?, ?)",
                             (1100, response, "received", "MCM"))
            (root / "status.json").write_text(json.dumps({"jobs": [{
                "mid": 123, "landing": 1000, "state": "uncertain_mcm",
                "sent_mid": 156018970, "confirmed_at": 900, "lid": 0,
                "error": "MCM injection outcome uncertain"}]}))
            support = AutoSupport(root / "config.json", root / "status.json")
            self.assertEqual(support.jobs[123]["state"], "recalled")
            self.assertEqual(support.jobs[123]["return_at"], 1405)
            self.assertNotIn("error", support.jobs[123])

    def test_missing_header_preflight_retries_recall_without_uncertain_state(self):
        with tempfile.TemporaryDirectory() as folder:
            support = AutoSupport(Path(folder) / "config.json", Path(folder) / "status.json")
            support.jobs[123] = {"mid": 123, "landing": 1100, "state": "sent",
                                 "sent_mid": 456, "lid": 0}
            attempts = []
            def send(command, payload):
                attempts.append((command, payload))
                if len(attempts) == 1:
                    raise SupportNotSent("header missing")
            with patch("backend.auto_support.time.time", return_value=1100):
                support.tick(send)
            self.assertEqual(support.jobs[123]["state"], "sent")
            with patch("backend.auto_support.time.time", return_value=1100.25):
                support.tick(send)
            self.assertEqual(len(attempts), 1)
            with patch("backend.auto_support.time.time", return_value=1100.5):
                support.tick(send)
            self.assertEqual(support.jobs[123]["state"], "await_mcm")
            self.assertEqual(len(attempts), 2)

    def test_manual_recall_response_prevents_automatic_duplicate(self):
        with tempfile.TemporaryDirectory() as folder:
            support = AutoSupport(Path(folder) / "config.json", Path(folder) / "status.json")
            support.jobs[123] = {"mid": 123, "landing": 1100, "state": "sent",
                                 "sent_mid": 156018970, "lid": 0}
            support.observe((ROOT / "tests" / "fixtures" / "mcm_response.txt").read_text(), 1000)
            self.assertEqual(support.jobs[123]["state"], "recalled")
            with patch("backend.auto_support.time.time", return_value=1101):
                support.tick(lambda *_: self.fail("duplicate recall"))


if __name__ == "__main__":
    unittest.main()
