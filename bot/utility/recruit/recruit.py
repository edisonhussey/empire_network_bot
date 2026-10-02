"""Pure recruitment protocol, navigation, and live-state helpers.

This module does not inject packets.  It provides the validated commands and
state transitions a scheduler/driver can execute one response at a time.  That
keeps the unverified multi-slot behaviour out of the live transport.

Database persistence is intentionally not defined here yet. The runtime model
must stabilise before its storage schema is made permanent.
"""

from __future__ import annotations

from dataclasses import dataclass, replace
import random
from typing import Any, Callable, Mapping

from bot.packets import castles_by_kingdom, parse_xt_packet, xt_packet
from bot.utility.recruit.config import (
    RECRUIT_TASKS,
    AccountRecruitPlan,
    RecruitTask,
)


@dataclass(frozen=True)
class CastleLocation:
    kingdom_id: int
    castle_id: int

    def __post_init__(self) -> None:
        if self.kingdom_id < 0 or self.castle_id <= 0:
            raise ValueError("invalid castle location")


@dataclass(frozen=True)
class OwnedCastle:
    location: CastleLocation
    area_type: int | None = None
    x: int | None = None
    y: int | None = None
    name: str = ""

    @property
    def is_green_main(self) -> bool:
        return self.location.kingdom_id == 0 and self.area_type == 1


@dataclass(frozen=True)
class NavigationState:
    current_kingdom: int | None
    current_castle_id: int | None
    map_mode: bool = True
    recruit_page: bool = False

    def __post_init__(self) -> None:
        if self.current_kingdom is not None and self.current_kingdom < 0:
            raise ValueError("current_kingdom cannot be negative")
        if self.current_castle_id is not None and self.current_castle_id <= 0:
            raise ValueError("current_castle_id must be positive")
        if self.recruit_page and self.map_mode:
            raise ValueError("recruit_page requires castle mode")
        if self.recruit_page and self.current_castle_id is None:
            raise ValueError("recruit_page requires a current castle")

    def enter_map(self, kingdom_id: int | None = None) -> "NavigationState":
        if kingdom_id is not None and kingdom_id < 0:
            raise ValueError("kingdom_id cannot be negative")
        return replace(
            self,
            current_kingdom=self.current_kingdom if kingdom_id is None else kingdom_id,
            map_mode=True,
            recruit_page=False,
        )

    def enter_castle(self, location: CastleLocation) -> "NavigationState":
        return NavigationState(
            current_kingdom=location.kingdom_id,
            current_castle_id=location.castle_id,
            map_mode=False,
            recruit_page=False,
        )

    def open_recruit_page(self) -> "NavigationState":
        if self.map_mode or self.current_castle_id is None:
            raise ValueError("cannot open recruitment while in map mode")
        return replace(self, recruit_page=True)

    def close_recruit_page(self) -> "NavigationState":
        return replace(self, recruit_page=False)

    def require_attack_ready(self) -> None:
        """Fail closed unless the browser is known to be in general map view."""

        if not self.map_mode:
            raise RuntimeError("attacks require general map view")

    def require_castle_ready(self) -> None:
        """Fail closed unless the browser is known to be inside an owned castle."""

        if self.map_mode or self.current_castle_id is None:
            raise RuntimeError("castle action requires an active owned castle")


@dataclass(frozen=True)
class NavigationPolicy:
    """Timing invariant for moving the client between owned castles."""

    minimum_castle_switch_seconds: float = 3.0

    def __post_init__(self) -> None:
        if self.minimum_castle_switch_seconds < 3.0:
            raise ValueError("castle switches require at least three seconds")

    def castle_switch_due_at(self, last_switch_at: float | None) -> float:
        if last_switch_at is None:
            return 0.0
        return float(last_switch_at) + self.minimum_castle_switch_seconds

    def can_switch_castle(self, *, now: float, last_switch_at: float | None) -> bool:
        return float(now) >= self.castle_switch_due_at(last_switch_at)


DEFAULT_NAVIGATION_POLICY = NavigationPolicy()


@dataclass(frozen=True)
class CastleDirectory:
    """Castle ID to location mapping learned from live server state."""

    locations: Mapping[int, CastleLocation]

    def location(self, castle_id: int) -> CastleLocation:
        try:
            return self.locations[int(castle_id)]
        except KeyError as exc:
            raise LookupError(f"castle {castle_id} is absent from live game state") from exc

    def merged(self, locations: Mapping[int, CastleLocation]) -> "CastleDirectory":
        result = dict(self.locations)
        result.update(locations)
        return CastleDirectory(result)


@dataclass(frozen=True)
class ResolvedRecruitment:
    task_id: str
    task: RecruitTask
    location: CastleLocation


@dataclass(frozen=True)
class CastleMovePlan:
    """A dynamically resolved JCA transition, including its earliest send time."""

    destination: CastleLocation
    required: bool
    due_at: float

    def packet(self, *, now: float) -> str | None:
        if not self.required:
            return None
        if float(now) < self.due_at:
            raise RuntimeError(f"castle movement is not due until {self.due_at:.3f}")
        return castle_packet(self.destination)


@dataclass(frozen=True)
class RecruitReceipt:
    lane_id: int
    troop_id: int
    active_quantity: int
    queued_quantity: int
    current_remaining_seconds: int
    total_remaining_seconds: int
    help_active: bool

    @property
    def total_quantity(self) -> int:
        return self.active_quantity + self.queued_quantity

    def queue_clear_at(
        self,
        received_at: float,
        *,
        buffer_seconds: int = 10,
    ) -> int:
        """Estimate when the server-reported lane queue clears.

        ``TCT`` is cumulative. Consecutive live BUP responses increased it from
        5409 to 8258, 11107 and 13955 seconds, so multiplying it by the desired
        slot count would count the same queued work repeatedly.
        """

        if buffer_seconds < 0:
            raise ValueError("invalid queue timing inputs")
        return int(received_at + self.total_remaining_seconds + buffer_seconds)


def castle_packet(location: CastleLocation) -> str:
    return xt_packet("jca", {"CID": location.castle_id, "KID": location.kingdom_id})


def plan_castle_move(
    state: NavigationState,
    destination: CastleLocation,
    *,
    last_switch_at: float | None,
    policy: NavigationPolicy = DEFAULT_NAVIGATION_POLICY,
) -> CastleMovePlan:
    """Plan movement from observed state without a configured return castle."""

    already_inside = (
        not state.map_mode
        and state.current_kingdom == destination.kingdom_id
        and state.current_castle_id == destination.castle_id
    )
    return CastleMovePlan(
        destination=destination,
        required=not already_inside,
        due_at=policy.castle_switch_due_at(last_switch_at) if not already_inside else 0.0,
    )


def recruit_page_packets(*, lane_id: int = 0) -> tuple[str, ...]:
    """Read-only page bootstrap observed immediately after Sands JCA."""

    return (
        xt_packet("dcl", {"CD": 0}),
        xt_packet("gpa", {}),
        xt_packet("spl", {"LID": int(lane_id)}),
        xt_packet("gui", {}),
    )


def recruit_packet(task: RecruitTask, location: CastleLocation) -> str:
    event = task.event
    return xt_packet(
        "bup",
        {
            "LID": event.lane_id,
            "WID": event.troop_id,
            "AMT": event.quantity,
            "PO": -1,
            "PWR": 0,
            "SK": event.skill_id,
            "SID": location.kingdom_id,
            "AID": location.castle_id,
        },
    )


def alliance_help_packet(*, lane_id: int = 0) -> str:
    return xt_packet("ahr", {"ID": int(lane_id), "T": 6})


def parse_recruit_state(payload: dict[str, Any]) -> RecruitReceipt:
    """Parse an SPL state object from either BUP or SPL server traffic."""

    spl = payload.get("spl") if isinstance(payload.get("spl"), dict) else payload
    if not isinstance(spl, dict):
        raise ValueError("response has no spl state")
    active = spl.get("PS") or {}
    queues = spl.get("QS") or []
    queued = 0
    for queue in queues:
        if not isinstance(queue, dict):
            continue
        queue_payload = queue.get("P") or {}
        if isinstance(queue_payload, dict):
            queued += int(queue_payload.get("TUA", 0) or 0)
    return RecruitReceipt(
        lane_id=int(spl.get("LID", 0)),
        troop_id=int(active.get("WID", 0)),
        active_quantity=int(active.get("TUA", 0) or 0),
        queued_quantity=queued,
        current_remaining_seconds=int(active.get("RCT", 0) or 0),
        total_remaining_seconds=int(spl.get("TCT", 0) or 0),
        help_active=bool(active.get("RAH", False)),
    )


def parse_recruit_response(packet: str | dict[str, Any]) -> RecruitReceipt:
    parsed = parse_xt_packet(packet) if isinstance(packet, str) else packet
    if not parsed or parsed.get("command") != "bup":
        raise ValueError("expected a BUP response")
    status = parsed.get("status")
    if status not in (None, "0", 0):
        raise ValueError(f"BUP failed with status {status}")
    payload = parsed.get("payload")
    if not isinstance(payload, dict):
        raise ValueError("BUP response has no object payload")
    return parse_recruit_state(payload)


def request_delays(
    task: RecruitTask,
    *,
    gaussian: Callable[[float, float], float] = random.gauss,
) -> tuple[float, ...]:
    """Sample bounded Gaussian gaps; this function never sends packets."""

    event = task.event
    low, high = event.request_delay_bounds
    return tuple(
        min(high, max(low, float(gaussian(event.request_delay_mean, event.request_delay_stddev))))
        for _ in range(event.slot_quantity - 1)
    )


def castle_locations_from_packet(packet: str | dict[str, Any]) -> dict[int, CastleLocation]:
    """Learn castle kingdoms from DCL, GCL/GBD, or JAA server state."""

    return {
        castle_id: castle.location
        for castle_id, castle in owned_castles_from_packet(packet).items()
    }


def owned_castles_from_packet(packet: str | dict[str, Any]) -> dict[int, OwnedCastle]:
    """Learn owned-castle identity and metadata from live server state."""

    parsed = parse_xt_packet(packet) if isinstance(packet, str) else packet
    if not parsed:
        return {}
    command = parsed.get("command")
    payload = parsed.get("payload")
    if not isinstance(payload, dict):
        return {}

    result: dict[int, OwnedCastle] = {}
    if command == "dcl":
        for block in payload.get("C") or []:
            if not isinstance(block, dict):
                continue
            try:
                kingdom_id = int(block["KID"])
            except (KeyError, TypeError, ValueError):
                continue
            for area in block.get("AI") or []:
                if not isinstance(area, dict):
                    continue
                try:
                    castle_id = int(area["AID"])
                except (KeyError, TypeError, ValueError):
                    continue
                result[castle_id] = OwnedCastle(CastleLocation(kingdom_id, castle_id))
        return result

    if command in {"gbd", "gcl"}:
        for kingdom_id, castles in castles_by_kingdom(payload).items():
            for castle in castles:
                castle_id = int(castle["castle_id"])
                result[castle_id] = OwnedCastle(
                    location=CastleLocation(int(kingdom_id), castle_id),
                    area_type=int(castle["type"]),
                    x=int(castle["x"]),
                    y=int(castle["y"]),
                    name=str(castle.get("name") or ""),
                )
        return result

    if command == "jaa":
        try:
            kingdom_id = int(payload["KID"])
            active = payload["gca"]["A"]
            castle_id = int(active[3])
        except (KeyError, IndexError, TypeError, ValueError):
            return {}
        result[castle_id] = OwnedCastle(
            location=CastleLocation(kingdom_id, castle_id),
            area_type=int(active[0]),
            x=int(active[1]),
            y=int(active[2]),
            name=str(active[10]) if len(active) > 10 else "",
        )
    return result


def observe_navigation_packet(
    state: NavigationState,
    packet: str | dict[str, Any],
) -> NavigationState:
    """Apply only navigation transitions confirmed by live packet state."""

    parsed = parse_xt_packet(packet) if isinstance(packet, str) else packet
    if not parsed:
        return state
    command = parsed.get("command")
    payload = parsed.get("payload")
    if not isinstance(payload, dict):
        return state

    if command == "gaa":
        try:
            return state.enter_map(int(payload["KID"]))
        except (KeyError, TypeError, ValueError):
            return state

    if command == "jaa" and parsed.get("status") in (None, "0", 0):
        locations = castle_locations_from_packet(parsed)
        if len(locations) == 1:
            return state.enter_castle(next(iter(locations.values())))
    return state


def resolve_plan(
    plan: AccountRecruitPlan,
    directory: CastleDirectory,
    *,
    task_registry: Mapping[str, RecruitTask] = RECRUIT_TASKS,
) -> tuple[ResolvedRecruitment, ...]:
    """Resolve account subscriptions using the latest live castle directory."""

    result: list[ResolvedRecruitment] = []
    for subscription in plan.subscriptions:
        try:
            task = task_registry[subscription.task_id]
        except KeyError as exc:
            raise LookupError(f"unknown recruitment task {subscription.task_id!r}") from exc
        for castle_id in subscription.castle_ids:
            result.append(
                ResolvedRecruitment(
                    task_id=subscription.task_id,
                    task=task,
                    location=directory.location(castle_id),
                )
            )
    return tuple(result)
