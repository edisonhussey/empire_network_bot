from __future__ import annotations

import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

from backend.storage import Recorder, event, events, export_session, set_meta


class DirectionStorageTests(unittest.TestCase):
    def test_new_session_keeps_both_directions_in_order(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            recorder = Recorder(root)
            session_id = recorder.start()
            recorder.capture('%xt%mrm%1%0%{"MID":155312686}%', 1000, "sent")
            recorder.capture('%xt%gam%1%0%{"M":[],"O":[]}%', 1001, "received")
            recorder.stop()
            recorder.close()

            rows = events(root, session_id, [], "", 100, 0)["events"]
            self.assertEqual([(row["type"], row["direction"]) for row in rows],
                             [("MRM", "sent"), ("GAM", "received")])
            self.assertEqual(event(root, session_id, 1)["direction"], "sent")
            exported = root / "export.json"
            export_session(root, session_id, exported)
            self.assertEqual([row["direction"] for row in json.loads(exported.read_text())["events"]],
                             ["sent", "received"])

    def test_old_session_reads_as_received(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            session_id = "a" * 32
            with sqlite3.connect(root / f"{session_id}.sqlite3") as conn:
                conn.executescript("CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);"
                                   "CREATE TABLE events(seq INTEGER PRIMARY KEY,received_at REAL NOT NULL,"
                                   "type TEXT NOT NULL,summary TEXT NOT NULL,raw TEXT NOT NULL,decoded TEXT NOT NULL);")
                set_meta(conn, version=1, id=session_id, started_at=1, ended_at=2, event_count=1)
                conn.execute("INSERT INTO events VALUES (1,1,'GAM','old','%xt%gam%1%0%{}%',?)",
                             (json.dumps({"command": "gam"}),))
            self.assertEqual(events(root, session_id, [], "", 100, 0)["events"][0]["direction"], "received")
            self.assertEqual(event(root, session_id, 1)["direction"], "received")
            exported = root / "export.json"
            export_session(root, session_id, exported)
            self.assertEqual(json.loads(exported.read_text())["events"][0]["direction"], "received")


if __name__ == "__main__":
    unittest.main()
