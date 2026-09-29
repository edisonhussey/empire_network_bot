import unittest

from run_berimond import build_argv


class RunBerimondConfigTests(unittest.TestCase):
    def test_time_skip_config_is_forwarded_to_controller(self):
        argv = build_argv(
            {
                "account": "ventrilo",
                "refill_time_skip": True,
                "refill_time_skip_type": "MS5",
                "refill_max_time_skips": 2,
            }
        )
        self.assertIn("--refill-time-skip", argv)
        self.assertEqual(argv[argv.index("--refill-time-skip-type") + 1], "MS5")
        self.assertEqual(argv[argv.index("--refill-max-time-skips") + 1], "2")

    def test_time_skip_can_be_disabled_explicitly(self):
        argv = build_argv({"account": "ventrilo", "refill_time_skip": False})
        self.assertIn("--no-refill-time-skip", argv)
        self.assertNotIn("--refill-time-skip", argv)


if __name__ == "__main__":
    unittest.main()
