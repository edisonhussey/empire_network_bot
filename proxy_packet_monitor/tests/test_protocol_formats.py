from __future__ import annotations

import unittest

from backend.protocol import parse_packet, summary


class PacketFormatTests(unittest.TestCase):
    def test_xml_system_message(self):
        frame = "<msg t='sys'><body action='verChk' r='0'><ver v='166' /></body></msg>"
        parsed = parse_packet(frame)
        self.assertEqual(parsed["command"], "verChk")
        self.assertEqual(parsed["format"], "xml")
        self.assertEqual(parsed["payload"]["children"][0]["children"][0]["attributes"], {"v": "166"})
        self.assertEqual(summary(parsed), "XML msg · verChk")

    def test_header_xt_without_payload(self):
        parsed = parse_packet("%xt%EmpireEx_49%nch%1%")
        self.assertEqual(parsed["command"], "nch")
        self.assertIsNone(parsed["payload"])

    def test_mrm_still_parses(self):
        parsed = parse_packet('%xt%mrm%1%0%{"MID":155312686}%')
        self.assertEqual(parsed["payload"], {"MID": 155312686})


if __name__ == "__main__":
    unittest.main()
