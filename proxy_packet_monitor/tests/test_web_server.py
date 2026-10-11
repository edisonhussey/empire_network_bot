from __future__ import annotations
import json
import socket
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock

from backend.web_server import DEFAULT_CONFIG, Handler, Service


class WebServerTests(unittest.TestCase):
    def test_proxy_port_conflict_is_reported(self):
        with tempfile.TemporaryDirectory() as folder, socket.socket() as occupied:
            occupied.bind(("127.0.0.1", 0))
            config = {**DEFAULT_CONFIG, "data_dir": str(Path(folder) / "data"),
                      "proxy_port": occupied.getsockname()[1]}
            path = Path(folder) / "config.json"
            path.write_text(json.dumps(config), encoding="utf-8")
            service = Service(path)
            with self.assertRaisesRegex(RuntimeError, "already in use"):
                service.start_proxy()
            self.assertIsNone(service.proxy)

    def test_cancelled_response_is_quiet(self):
        handler = Handler.__new__(Handler)
        handler.send_response = Mock()
        handler.send_header = Mock()
        handler.end_headers = Mock(side_effect=BrokenPipeError())
        handler.wfile = Mock()
        handler._reply(200, {"ok": True})


if __name__ == "__main__":
    unittest.main()
