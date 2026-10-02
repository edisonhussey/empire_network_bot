import unittest

from bot.proxy_bot import parse_args


class ProxyBotCliTests(unittest.TestCase):
    def test_recruit_uses_account_default_when_not_overridden(self) -> None:
        args = parse_args(["--ventrilo", "--start", "--mode", "sands"])
        self.assertIsNone(args.recruit)

    def test_recruit_accepts_explicit_boolean_override(self) -> None:
        enabled = parse_args(
            ["--ventrilo", "--start", "--mode", "sands", "--recruit", "true"]
        )
        disabled = parse_args(
            ["--ventrilo", "--start", "--mode", "storm", "--recruit", "false"]
        )
        self.assertEqual(enabled.recruit, "true")
        self.assertEqual(disabled.recruit, "false")


if __name__ == "__main__":
    unittest.main()
