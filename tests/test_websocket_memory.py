import unittest

from bot.websocket_memory import trim_websocket_history


class FakeWebSocket:
    def __init__(self, messages):
        self.messages = list(messages)


class WebSocketMemoryTests(unittest.TestCase):
    def test_history_is_bounded_and_newest_messages_survive(self) -> None:
        websocket = FakeWebSocket(range(20))
        self.assertEqual(trim_websocket_history(websocket, limit=4), 16)
        self.assertEqual(websocket.messages, [16, 17, 18, 19])

    def test_short_history_is_unchanged(self) -> None:
        websocket = FakeWebSocket([1, 2])
        self.assertEqual(trim_websocket_history(websocket, limit=4), 0)
        self.assertEqual(websocket.messages, [1, 2])

    def test_zero_limit_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            trim_websocket_history(FakeWebSocket([]), limit=0)


if __name__ == "__main__":
    unittest.main()
