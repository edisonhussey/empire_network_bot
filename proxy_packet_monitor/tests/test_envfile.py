from __future__ import annotations
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from backend.envfile import overrides, update_env


class EnvFileTests(unittest.TestCase):
    def test_env_file_then_real_environment_wins(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / ".env"
            path.write_text("# c\nPPM_WEB_PORT=8008\nPPM_PROXY_PORT=\nPPM_WEB_HOST='0.0.0.0'\n", encoding="utf-8")
            with patch.dict(os.environ, {}, clear=True):
                self.assertEqual(overrides(path), {"api_port": 8008, "api_host": "0.0.0.0"})
            with patch.dict(os.environ, {"PPM_WEB_PORT": "9000"}, clear=True):
                self.assertEqual(overrides(path)["api_port"], 9000)

    def test_bad_port_names_the_key(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / ".env"
            path.write_text("PPM_WEB_PORT=abc\n", encoding="utf-8")
            with patch.dict(os.environ, {}, clear=True), self.assertRaisesRegex(ValueError, "PPM_WEB_PORT"):
                overrides(path)

    def test_update_keeps_comments_and_appends(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / ".env"
            path.write_text("# note\nPPM_WEB_PORT=8008\n", encoding="utf-8")
            update_env(path, {"PPM_WEB_PORT": "8009", "PPM_PROXY_PORT": "8081"})
            self.assertEqual(path.read_text(encoding="utf-8"), "# note\nPPM_WEB_PORT=8009\nPPM_PROXY_PORT=8081\n")


if __name__ == "__main__":
    unittest.main()
