"""Declarative recruitment plans.

Only values observed in the Ventrilo Sands capture are enabled here.  Other
accounts can subscribe by adding an :class:`AccountRecruitPlan`; the protocol
code remains account-independent.
"""

from __future__ import annotations

from dataclasses import dataclass

from bot.game_data.troops import Troop


@dataclass(frozen=True)
class CastleRecruitEvent:
    troop: Troop
    quantity: int
    slot_quantity: int = 1
    ask_help: bool = True
    lane_id: int = 0
    skill_id: int = 73
    request_delay_mean: float = 3.1
    request_delay_stddev: float = 0.8
    request_delay_bounds: tuple[float, float] = (0.5, 6.0)
    ready_buffer_seconds: int = 10

    def __post_init__(self) -> None:
        if self.quantity <= 0:
            raise ValueError("quantity must be positive")
        if self.slot_quantity <= 0:
            raise ValueError("slot_quantity must be positive")
        low, high = self.request_delay_bounds
        if low < 0.5 or high < low:
            raise ValueError("request delay bounds must be ordered and at least 0.5 seconds")
        if self.request_delay_stddev < 0:
            raise ValueError("request delay deviation cannot be negative")
        if not low <= self.request_delay_mean <= high:
            raise ValueError("request delay mean must be within its bounds")
        if self.ready_buffer_seconds < 0:
            raise ValueError("ready buffer cannot be negative")

    @property
    def troop_id(self) -> int:
        return int(self.troop.id)


@dataclass(frozen=True)
class RecruitTask:
    event: CastleRecruitEvent


@dataclass(frozen=True)
class CastleTaskSubscription:
    """One task ID subscribed by one or more account-owned castles."""

    task_id: str
    castle_ids: tuple[int, ...]

    def __post_init__(self) -> None:
        if not self.task_id.strip():
            raise ValueError("task_id cannot be empty")
        if not self.castle_ids:
            raise ValueError("a subscription requires at least one castle")
        if any(castle_id <= 0 for castle_id in self.castle_ids):
            raise ValueError("castle IDs must be positive")
        if len(set(self.castle_ids)) != len(self.castle_ids):
            raise ValueError("castle IDs cannot be repeated in one subscription")


@dataclass(frozen=True)
class AccountRecruitPlan:
    account: str
    subscriptions: tuple[CastleTaskSubscription, ...]
    enabled_by_default: bool = True
    # BUP carries both the destination kingdom and castle IDs, so recruitment
    # can run without JCA-changing the player's visible screen. AHR does not
    # carry that context and is therefore skipped in background mode.
    background_recruitment: bool = True
    auto_return_to_map: bool = False
    map_button_relative: tuple[float, float] = (0.70, 0.965)

    def __post_init__(self) -> None:
        task_ids = [subscription.task_id for subscription in self.subscriptions]
        duplicates = sorted({task_id for task_id in task_ids if task_ids.count(task_id) > 1})
        if duplicates:
            raise ValueError(f"task IDs subscribed more than once: {duplicates}")
        if any(not 0.0 < value < 1.0 for value in self.map_button_relative):
            raise ValueError("map button coordinates must be relative window positions")

    def castle_ids_for(self, task_id: str) -> tuple[int, ...]:
        for subscription in self.subscriptions:
            if subscription.task_id == task_id:
                return subscription.castle_ids
        return ()


# A task contains only the event. Castle and kingdom selection are runtime
# concerns resolved from the account's live castle directory.
# Stable event name: quantity and slot count are event configuration, not task
# identity, so changing 190x1 to 180x5 does not require a new subscription ID.
SAND_CROSSBOW_TASK_ID = "sand_crossbowman"
SAND_CROSSBOW_RECRUIT = RecruitTask(
    event=CastleRecruitEvent(
        troop=Troop.CROSSBOWMAN,
        quantity=170,
        slot_quantity=5,
        ask_help=True,
    )
)

RECRUIT_TASKS: dict[str, RecruitTask] = {
    SAND_CROSSBOW_TASK_ID: SAND_CROSSBOW_RECRUIT,
}

ACCOUNT_RECRUIT_PLANS: dict[str, AccountRecruitPlan] = {
    "ventrilo": AccountRecruitPlan(
        account="ventrilo",
        subscriptions=(
            CastleTaskSubscription(
                task_id=SAND_CROSSBOW_TASK_ID,
                castle_ids=(
                    16_011_862,
                    16_632_819,
                    16_673_031,
                    16_366_514,
                    16_366_513,
                    16_681_051,
                ),
            ),
        ),
    )
}


def plan_for_account(account: str) -> AccountRecruitPlan | None:
    return ACCOUNT_RECRUIT_PLANS.get(account.casefold())


def task_for_id(task_id: str) -> RecruitTask:
    try:
        return RECRUIT_TASKS[task_id]
    except KeyError as exc:
        raise KeyError(f"unknown recruitment task {task_id!r}") from exc
