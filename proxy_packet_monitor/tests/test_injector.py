from __future__ import annotations
import json
import tempfile
import threading
import unittest
from pathlib import Path

from backend.injector import InjectChannel, InjectError, MAX_FRAMES, split_frames


class SplitFramesTests(unittest.TestCase):
    def test_one_frame_per_line_untouched_inside(self):
        text = "  %xt%EmpireEx_19%nch%1%  \n\n%xt%EmpireEx_19%pin%1%{\"a\": 1}%\r\n"
        self.assertEqual(split_frames(text), ["%xt%EmpireEx_19%nch%1%", '%xt%EmpireEx_19%pin%1%{"a": 1}%'])

    def test_nothing_or_too_much_is_refused(self):
        for bad in ("", "  \n ", None, 5):
            with self.assertRaises(InjectError):
                split_frames(bad)
        with self.assertRaises(InjectError):
            split_frames("\n".join("%xt%x%a%1%" for _ in range(MAX_FRAMES + 1)))
        with self.assertRaises(InjectError):
            split_frames("x" * 70_000)


class ChannelTests(unittest.TestCase):
    def setUp(self):
        self._folder = tempfile.TemporaryDirectory()
        self.folder = Path(self._folder.name)

    def tearDown(self):
        self._folder.cleanup()

    def serve(self, channel, send, rounds=100):
        def run():
            import time
            for _ in range(rounds):
                channel.tick(send)
                time.sleep(0.01)
        thread = threading.Thread(target=run)
        thread.start()
        return thread

    def test_frames_reach_the_sender_in_order_with_the_direction(self):
        seen = []
        proxy = InjectChannel(self.folder)
        thread = self.serve(proxy, lambda frame, to_client: seen.append((frame, to_client)))
        result = InjectChannel(self.folder).submit("%xt%EmpireEx_19%nch%1%\n%xt%EmpireEx_19%pin%1%", to_client=True)
        thread.join()
        self.assertEqual(result, {"sent": 2, "to_client": True})
        self.assertEqual(seen, [("%xt%EmpireEx_19%nch%1%", True), ("%xt%EmpireEx_19%pin%1%", True)])

    def test_a_refusal_comes_back_as_the_error_and_counts_what_went_out(self):
        def send(frame, to_client):
            if frame.endswith("bad%"):
                raise RuntimeError("No active game WebSocket.")
        proxy = InjectChannel(self.folder)
        thread = self.serve(proxy, send)
        with self.assertRaisesRegex(InjectError, r"No active game WebSocket\. \(sent 1 before that\)"):
            InjectChannel(self.folder).submit("%xt%a%ok%1%\n%xt%a%bad%")
        thread.join()

    def test_a_request_left_by_an_earlier_run_is_not_sent_again(self):
        InjectChannel(self.folder).request.write_text(json.dumps({"id": "old", "frames": ["%xt%a%b%1%"]}))
        seen = []
        proxy = InjectChannel(self.folder)  # starts after the leftover
        proxy.tick(lambda frame, to_client: seen.append(frame))
        self.assertEqual(seen, [])

    def test_each_request_is_sent_once(self):
        seen = []
        proxy = InjectChannel(self.folder)
        page = InjectChannel(self.folder)
        thread = self.serve(proxy, lambda frame, to_client: seen.append(frame), rounds=60)
        page.submit("%xt%a%one%1%")
        thread.join()
        proxy.tick(lambda frame, to_client: seen.append(frame))
        self.assertEqual(seen, ["%xt%a%one%1%"])

    def test_no_proxy_means_a_clear_timeout(self):
        with self.assertRaisesRegex(InjectError, "did not answer"):
            InjectChannel(self.folder).submit("%xt%a%b%1%", wait_s=0.2)


if __name__ == "__main__":
    unittest.main()
