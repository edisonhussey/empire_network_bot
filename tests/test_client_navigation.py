import unittest

from bot.client_navigation import MapButtonTarget, chrome_map_click_script


class ClientNavigationTests(unittest.TestCase):
    def test_script_targets_only_game_window_and_relative_button(self) -> None:
        script = chrome_map_click_script(MapButtonTarget(relative_x=0.7, relative_y=0.965))
        self.assertIn('gameURL contains "empire.goodgamestudios.com"', script)
        self.assertIn("* 0.7", script)
        self.assertIn("* 0.965", script)
        self.assertIn("click at", script)

    def test_relative_coordinates_are_validated(self) -> None:
        with self.assertRaises(ValueError):
            MapButtonTarget(relative_x=1.0)


if __name__ == "__main__":
    unittest.main()
