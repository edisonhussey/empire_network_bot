import random
import unittest
from types import SimpleNamespace

from bot import packets
from bot.event.storm import config, controller as storm


class _Task:
    kingdom_id = 4
    target_levels = (60, 80)
    target_level = None

    @staticmethod
    def attack_payload():
        return [{"L": {"U": [[10, 20]]}}]


class StormTests(unittest.TestCase):
    def test_explicit_levels_override_task_levels(self):
        self.assertEqual(storm.target_levels(_Task(), "80, 60,80"), (60, 80))
        self.assertEqual(storm.target_levels(_Task(), [70, 60]), (60, 70))

    def test_task_levels_are_used_when_no_override_is_present(self):
        self.assertEqual(storm.target_levels(_Task(), None), (60, 80))

    def test_default_storm_task_is_available_outside_sands_subscriptions(self):
        task = storm.active_task("storm")
        self.assertEqual(task.name, "storm")
        self.assertEqual(task.target_levels, (60, 70, 80))
        self.assertEqual(len(task.commander_lids), 14)

    def test_storm_source_is_dynamic_and_example_army_is_configured(self):
        self.assertEqual((config.SOURCE_X, config.SOURCE_Y), (None, None))
        payload = config.STORM_RENEGADE_KUNAI_FLANKS.to_payload()
        self.assertEqual(len(payload), 4)
        for attack_wave in payload:
            self.assertEqual(attack_wave["L"]["U"][0], [35, 50])
            self.assertEqual(attack_wave["R"]["U"][0], [35, 50])
            self.assertEqual(attack_wave["M"]["U"][0], [-1, 0])

    def test_explicit_source_override_is_paired(self):
        args = SimpleNamespace(source_x=521, source_y=591)
        self.assertEqual(storm.resolve_source(args), (521, 591, "cli_override"))

        with self.assertRaisesRegex(RuntimeError, "requires both"):
            storm.resolve_source(SimpleNamespace(source_x=521, source_y=None))

    def test_scan_chunks_are_aligned_and_non_negative(self):
        random.seed(7)
        chunks = storm.scan_chunks(20, 20, 52)
        self.assertTrue(chunks)
        for x, y in chunks:
            self.assertGreaterEqual(x, 0)
            self.assertGreaterEqual(y, 0)
            self.assertEqual(x % packets.MAP_CHUNK_SIZE, 0)
            self.assertEqual(y % packets.MAP_CHUNK_SIZE, 0)

    def test_attack_payload_uses_storm_task_and_source(self):
        payload = storm.build_attack_payload(
            {"x": 700, "y": 710},
            8,
            _Task(),
            source_x=675,
            source_y=676,
            hbw=-1,
            ptt=1,
        )
        self.assertEqual((payload["SX"], payload["SY"]), (675, 676))
        self.assertEqual((payload["TX"], payload["TY"]), (700, 710))
        self.assertEqual(payload["KID"], 4)
        self.assertEqual(payload["LID"], 8)
        self.assertEqual(payload["A"], _Task.attack_payload())

    def test_scan_wait_keeps_randomizer_as_a_floor(self):
        args = SimpleNamespace(scan_min=4, scan_max=4, use_randomizer_gaa_wait=True)
        randomizer = SimpleNamespace(gaa_waiting_time=lambda: 9.5)
        self.assertGreaterEqual(storm.scan_wait_seconds(args, randomizer), 9.5)


if __name__ == "__main__":
    unittest.main()
