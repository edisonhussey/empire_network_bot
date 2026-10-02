import json
import unittest

from bot.packets import parse_xt_packet
from bot.utility.recruit.config import (
    ACCOUNT_RECRUIT_PLANS,
    SAND_CROSSBOW_RECRUIT,
    SAND_CROSSBOW_TASK_ID,
)
from bot.utility.recruit.recruit import (
    CastleDirectory,
    CastleLocation,
    NavigationPolicy,
    NavigationState,
    alliance_help_packet,
    castle_locations_from_packet,
    castle_packet,
    observe_navigation_packet,
    owned_castles_from_packet,
    parse_recruit_response,
    plan_castle_move,
    recruit_packet,
    recruit_page_packets,
    request_delays,
    resolve_plan,
)


def payload(packet: str) -> dict:
    parsed = parse_xt_packet(packet)
    assert parsed is not None
    return parsed["payload"]


class RecruitTests(unittest.TestCase):
    def test_observed_sands_packets_are_reproduced(self) -> None:
        task = SAND_CROSSBOW_RECRUIT
        location = CastleLocation(1, 16_366_514)
        self.assertEqual(
            payload(castle_packet(location)),
            {"CID": 16_366_514, "KID": 1},
        )
        self.assertEqual(
            payload(recruit_packet(task, location)),
            {
                "LID": 0,
                "WID": 607,
                "AMT": task.event.quantity,
                "PO": -1,
                "PWR": 0,
                "SK": 73,
                "SID": 1,
                "AID": 16_366_514,
            },
        )
        self.assertEqual(payload(alliance_help_packet()), {"ID": 0, "T": 6})

    def test_recruit_page_bootstrap_is_exact(self) -> None:
        parsed = [parse_xt_packet(packet) for packet in recruit_page_packets()]
        self.assertEqual(
            [(item["command"], item["payload"]) for item in parsed],
            [
                ("dcl", {"CD": 0}),
                ("gpa", {}),
                ("spl", {"LID": 0}),
                ("gui", {}),
            ],
        )

    def test_parse_observed_bup_response_and_schedule(self) -> None:
        response = {
            "spl": {
                "PS": {"WID": 607, "TUA": 5, "RAH": False, "RCT": 75},
                "QS": [{"P": {"WID": 607, "TUA": 185}}],
                "TCT": 2850,
                "LID": 0,
            }
        }
        packet = f"%xt%bup%1%0%{json.dumps(response, separators=(',', ':'))}%"
        receipt = parse_recruit_response(packet)
        self.assertEqual(receipt.troop_id, 607)
        self.assertEqual(receipt.total_quantity, 190)
        self.assertEqual(receipt.current_remaining_seconds, 75)
        self.assertEqual(receipt.queue_clear_at(1000, buffer_seconds=10), 3860)

    def test_navigation_invariant_and_transitions(self) -> None:
        state = NavigationState(current_kingdom=1, current_castle_id=None)
        castle = state.enter_castle(CastleLocation(1, 16_366_514))
        recruiting = castle.open_recruit_page()
        self.assertTrue(recruiting.recruit_page)
        self.assertFalse(recruiting.close_recruit_page().recruit_page)
        self.assertTrue(recruiting.enter_map().map_mode)
        with self.assertRaisesRegex(ValueError, "recruit_page requires castle mode"):
            NavigationState(1, 16_366_514, map_mode=True, recruit_page=True)

        with self.assertRaisesRegex(RuntimeError, "general map view"):
            recruiting.require_attack_ready()
        map_state = recruiting.enter_map()
        map_state.require_attack_ready()
        recruiting.require_castle_ready()
        with self.assertRaisesRegex(RuntimeError, "active owned castle"):
            map_state.require_castle_ready()

    def test_castle_switches_have_a_three_second_floor(self) -> None:
        policy = NavigationPolicy()
        self.assertFalse(policy.can_switch_castle(now=102.999, last_switch_at=100.0))
        self.assertTrue(policy.can_switch_castle(now=103.0, last_switch_at=100.0))
        with self.assertRaisesRegex(ValueError, "at least three seconds"):
            NavigationPolicy(minimum_castle_switch_seconds=2.999)

        state = NavigationState(0, 16_011_862, map_mode=False)
        move = plan_castle_move(
            state,
            CastleLocation(1, 16_366_514),
            last_switch_at=100.0,
        )
        with self.assertRaisesRegex(RuntimeError, "not due"):
            move.packet(now=102.999)
        self.assertEqual(
            payload(move.packet(now=103.0)),
            {"CID": 16_366_514, "KID": 1},
        )

    def test_no_castle_move_is_planned_when_already_inside_target(self) -> None:
        state = NavigationState(1, 16_366_514, map_mode=False)
        move = plan_castle_move(
            state,
            CastleLocation(1, 16_366_514),
            last_switch_at=100.0,
        )
        self.assertFalse(move.required)
        self.assertIsNone(move.packet(now=100.0))

    def test_account_plan_resolves_kingdom_from_live_dcl_state(self) -> None:
        parsed = {
            "command": "dcl",
            "payload": {
                "C": [
                    {
                        "KID": 0,
                        "AI": [
                            {"AID": 16_011_862},
                            {"AID": 16_632_819},
                            {"AID": 16_673_031},
                        ],
                    },
                    {"KID": 1, "AI": [{"AID": 16_366_514}]},
                    {"KID": 2, "AI": [{"AID": 16_366_513}]},
                    {"KID": 3, "AI": [{"AID": 16_681_051}]},
                ]
            },
        }
        directory = CastleDirectory(castle_locations_from_packet(parsed))
        resolved = resolve_plan(ACCOUNT_RECRUIT_PLANS["ventrilo"], directory)
        self.assertTrue(ACCOUNT_RECRUIT_PLANS["ventrilo"].enabled_by_default)
        self.assertTrue(ACCOUNT_RECRUIT_PLANS["ventrilo"].background_recruitment)
        self.assertFalse(ACCOUNT_RECRUIT_PLANS["ventrilo"].auto_return_to_map)
        self.assertEqual(len(resolved), 6)
        self.assertTrue(all(item.task_id == SAND_CROSSBOW_TASK_ID for item in resolved))
        self.assertTrue(all(item.task is SAND_CROSSBOW_RECRUIT for item in resolved))
        self.assertIn(CastleLocation(1, 16_366_514), [item.location for item in resolved])

    def test_login_gbd_learns_full_castle_metadata(self) -> None:
        parsed = {
            "command": "gbd",
            "status": "0",
            "payload": {
                "gcl": {
                    "C": [
                        {
                            "KID": 0,
                            "AI": [
                                {"AI": [1, 509, 405, 16_011_862, 1, 0, 0, 0, 0, 0, "._."]},
                                {"AI": [4, 505, 407, 16_632_819, 1, 0, 0, 0, 0, 0, "vrvr"]},
                            ],
                        },
                        {
                            "KID": 1,
                            "AI": [
                                {
                                    "AI": [
                                        12,
                                        593,
                                        613,
                                        16_366_514,
                                        1,
                                        0,
                                        0,
                                        0,
                                        0,
                                        0,
                                        "Castle Ventrilo",
                                    ]
                                }
                            ],
                        },
                    ]
                }
            },
        }
        castles = owned_castles_from_packet(parsed)
        self.assertEqual(set(castles), {16_011_862, 16_632_819, 16_366_514})
        self.assertTrue(castles[16_011_862].is_green_main)
        self.assertEqual(castles[16_366_514].location.kingdom_id, 1)
        self.assertEqual(castles[16_366_514].name, "Castle Ventrilo")
        self.assertEqual((castles[16_366_514].x, castles[16_366_514].y), (593, 613))

    def test_jaa_updates_active_castle_location(self) -> None:
        parsed = {
            "command": "jaa",
            "payload": {
                "KID": 1,
                "gca": {"A": [12, 593, 613, 16_366_514]},
            },
        }
        self.assertEqual(
            castle_locations_from_packet(parsed),
            {16_366_514: CastleLocation(1, 16_366_514)},
        )
        state = observe_navigation_packet(NavigationState(None, None), parsed)
        self.assertEqual(state, NavigationState(1, 16_366_514, map_mode=False))
        map_state = observe_navigation_packet(
            state,
            {"command": "gaa", "payload": {"KID": 1}},
        )
        self.assertTrue(map_state.map_mode)
        map_state.require_attack_ready()

    def test_slot_delays_are_bounded_but_do_not_send_packets(self) -> None:
        task = SAND_CROSSBOW_RECRUIT
        self.assertEqual(
            request_delays(task, gaussian=lambda mean, stddev: mean),
            (3.1, 3.1, 3.1, 3.1),
        )


if __name__ == "__main__":
    unittest.main()
