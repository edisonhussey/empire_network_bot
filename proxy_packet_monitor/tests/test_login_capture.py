from __future__ import annotations
import json
import tempfile
import unittest
from pathlib import Path

from backend.login_capture import LoginCapture

ITEMS = "https://empire-html5.goodgamestudios.com/default/items/items_1.json?v=2"


class LoginCaptureTests(unittest.TestCase):
    def make(self, folder, enabled=True):
        flag = Path(folder) / "flag.json"
        flag.write_text(json.dumps({"enabled": enabled}), encoding="utf-8")
        return LoginCapture(Path(folder) / "login_data", flag)

    def test_flag_and_filters(self):
        with tempfile.TemporaryDirectory() as folder:
            capture = self.make(folder, enabled=False)
            self.assertFalse(capture.enabled())
            capture.flag_path.write_text('{"enabled": true}', encoding="utf-8")
            self.assertTrue(capture.enabled())
            self.assertTrue(capture.wants("GET", ITEMS, 200, "application/json"))
            self.assertFalse(capture.wants("POST", ITEMS, 200, "application/json"))
            self.assertFalse(capture.wants("GET", ITEMS, 304, "application/json"))
            self.assertFalse(capture.wants("GET", ITEMS, 200, "image/png"))
            self.assertFalse(capture.wants("GET", "https://empire-html5.goodgamestudios.com/a/b.png", 200, ""))
            self.assertFalse(capture.wants("GET", "https://ads.example.com/x.js", 200, "text/javascript"))

    def test_save_dedupe_and_versions(self):
        with tempfile.TemporaryDirectory() as folder:
            capture = self.make(folder)
            first = capture.save(ITEMS, 200, "application/json", b'{"a":1}', 1.0)
            again = capture.save(ITEMS, 200, "application/json", b'{"a":1}', 2.0)
            changed = capture.save(ITEMS, 200, "application/json", b'{"a":2}', 3.0)
            self.assertEqual(first["file"], "files/empire-html5.goodgamestudios.com/default/items/items_1.json")
            self.assertTrue(first["new"])
            self.assertFalse(again["new"])
            self.assertEqual(again["file"], first["file"])
            self.assertTrue(changed["new"])
            self.assertRegex(changed["file"], r"items_1\.json\.[0-9a-f]{8}$")
            self.assertEqual((capture.root / first["file"]).read_bytes(), b'{"a":1}')
            self.assertEqual(len(capture.index.read_text(encoding="utf-8").splitlines()), 3)
            self.assertEqual(capture.saved, 2)

    def test_path_cannot_escape_and_state_survives_restart(self):
        with tempfile.TemporaryDirectory() as folder:
            capture = self.make(folder)
            entry = capture.save("https://empire-html5.goodgamestudios.com/../../../etc/x.json", 200, "", b"{}", 1.0)
            self.assertTrue((capture.root / entry["file"]).resolve().is_relative_to(capture.files.resolve()))
            capture.on_websocket("wss://ep-live-world2-game.goodgamestudios.com/")
            reopened = LoginCapture(capture.root, capture.flag_path)
            self.assertEqual(reopened.ws_seq, 1)
            self.assertFalse(reopened.save("https://empire-html5.goodgamestudios.com/y.json", 200, "", b"{}", 2.0)["new"])


if __name__ == "__main__":
    unittest.main()
