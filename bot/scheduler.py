from __future__ import annotations

import random
import sys
import time
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Iterable, Sequence


if __package__ in {None, ""}:
    REPO_ROOT = Path(__file__).resolve().parents[1]
    if str(REPO_ROOT) not in sys.path:
        sys.path.insert(0, str(REPO_ROOT))
    from game_data import Attack, KINGDOM, TOOL, TROOP, Kingdom, side, wave
    from ranomizer import Randomizer
else:
    from .game_data import Attack, KINGDOM, TOOL, TROOP, Kingdom, side, wave
    from .ranomizer import Randomizer


CommanderPool = tuple[int, ...]
TargetLevels = tuple[int, ...]


class AllocationMode(str, Enum):
    CUSTOM = "custom"
    GREEDY = "greedy"


def kingdom_id(kingdom: Kingdom | int) -> int:
    return kingdom.id if hasattr(kingdom, "id") else int(kingdom)


COMMANDER_LIDS_BY_HUMAN_NUMBER: CommanderPool = (
    0,
    2,
    3,
    6,
    7,
    8,
    9,
    10,
    11,
    16,
    17,
    18,
    20,
    21,
    22,
    23,
    24,
    25,
    26,
    27,
    28,
    29,
    30,
    31,
    32,
    33,
    34,
    35,
    36,
    37,
    38,
    39,
    40,
    41,
    42,
)


def commander_lid(human_number: int) -> int:
    """Map visible commander number to the server LID from live ADI/GAA rows."""

    if human_number < 1:
        raise ValueError("commander numbers are 1-based")
    try:
        return COMMANDER_LIDS_BY_HUMAN_NUMBER[human_number - 1]
    except IndexError as exc:
        raise ValueError(f"unknown commander number {human_number}") from exc


def commander_human_number(lid: int | None) -> int | None:
    if lid is None:
        return None
    try:
        return COMMANDER_LIDS_BY_HUMAN_NUMBER.index(int(lid)) + 1
    except ValueError:
        return None


def commander_range(first: int, last: int) -> CommanderPool:
    if last < first:
        raise ValueError("commander range end must be >= start")
    return tuple(commander_lid(number) for number in range(first, last + 1))


def commander_list(*human_numbers: int) -> CommanderPool:
    return tuple(commander_lid(number) for number in human_numbers)


@dataclass(frozen=True)
class Task:
    """Manual definition of one repeatable bot job.

    A task is not a sender. It describes a queue:
    - which kingdom it belongs to
    - what target shape it accepts
    - which commanders it may consume
    - which attack payload should be sent if live checks pass
    """

    name: str
    kingdom: Kingdom | int
    attack: Attack
    target_level: int | None = None
    target_levels: TargetLevels = ()
    commander_lids: CommanderPool = ()
    allocation: AllocationMode = AllocationMode.GREEDY
    max_active: int = 1
    priority: int = 100
    enabled: bool = True
    tags: tuple[str, ...] = ()
    notes: str = ""

    @property
    def kingdom_id(self) -> int:
        return kingdom_id(self.kingdom)

    def attack_payload(self) -> list[dict]:
        return self.attack.to_payload()

    def accepts_lid(self, lid: int) -> bool:
        return not self.commander_lids or int(lid) in self.commander_lids

    def accepts_level(self, level: int | None) -> bool:
        if level is None:
            return False
        if self.target_levels:
            return int(level) in self.target_levels
        if self.target_level is not None:
            return int(level) == int(self.target_level)
        return True


@dataclass(frozen=True)
class KingdomPlan:
    kingdom: Kingdom | int
    tasks: tuple[Task, ...]
    timeout_seconds: float
    started_at: float

    @property
    def kingdom_id(self) -> int:
        return kingdom_id(self.kingdom)

    @property
    def expires_at(self) -> float:
        return self.started_at + self.timeout_seconds

    def expired(self, now: float | None = None) -> bool:
        return (time.time() if now is None else now) >= self.expires_at


@dataclass(frozen=True)
class TaskDefinition:
    name: str
    kingdom: Kingdom | int
    attack: Attack
    commander_count: int
    target_level: int | None = None
    target_levels: TargetLevels = ()
    allocation: AllocationMode = AllocationMode.GREEDY
    max_active: int | None = None
    priority: int = 100
    enabled: bool = True
    tags: tuple[str, ...] = ()
    notes: str = ""


@dataclass
class Scheduler:
    tasks: list[Task] = field(default_factory=list)
    kingdom_order: tuple[Kingdom | int, ...] = (KINGDOM.sand,)
    kingdom_timeout: float = 300.0
    current_kingdom: Kingdom | int = KINGDOM.sand
    randomizer: Randomizer = field(default_factory=Randomizer)
    current_started_at: float = field(default_factory=time.time)

    def enabled_tasks(self) -> list[Task]:
        return [task for task in self.tasks if task.enabled]

    def by_name(self, name: str) -> Task:
        for task in self.tasks:
            if task.name == name:
                return task
        raise KeyError(name)

    def for_kingdom(self, kingdom: Kingdom | int) -> list[Task]:
        kid = kingdom_id(kingdom)
        return sorted(
            [task for task in self.enabled_tasks() if task.kingdom_id == kid],
            key=lambda task: task.priority,
        )

    def current_tasks(self) -> list[Task]:
        return self.for_kingdom(self.current_kingdom)

    def timeout_with_padding(self) -> float:
        return self.kingdom_timeout + max(0.0, self._random_padding())

    def current_plan(self) -> KingdomPlan:
        return KingdomPlan(
            kingdom=self.current_kingdom,
            tasks=tuple(self.current_tasks()),
            timeout_seconds=self.timeout_with_padding(),
            started_at=self.current_started_at,
        )

    def rotate_kingdom(self, *, now: float | None = None) -> Kingdom | int:
        if not self.kingdom_order:
            raise ValueError("kingdom_order cannot be empty")
        current_id = kingdom_id(self.current_kingdom)
        ids = [kingdom_id(kingdom) for kingdom in self.kingdom_order]
        try:
            index = ids.index(current_id)
        except ValueError:
            index = -1
        self.current_kingdom = self.kingdom_order[(index + 1) % len(self.kingdom_order)]
        self.current_started_at = time.time() if now is None else now
        return self.current_kingdom

    def task_for_lid(self, lid: int, kingdom: Kingdom | int | None = None) -> Task | None:
        candidates = self.current_tasks() if kingdom is None else self.for_kingdom(kingdom)
        for task in candidates:
            if task.accepts_lid(lid):
                return task
        return None

    def greedy_queue(self, available_lids: Iterable[int], kingdom: Kingdom | int | None = None) -> list[tuple[Task, int]]:
        """Assign available LIDs to tasks in priority order.

        This does not send anything. It returns the task/LID pairs a runner can
        attempt while it is already in that kingdom.
        """

        candidates = self.current_tasks() if kingdom is None else self.for_kingdom(kingdom)
        active_counts = {task.name: 0 for task in candidates}
        queue: list[tuple[Task, int]] = []
        for lid in sorted(set(int(value) for value in available_lids)):
            for task in candidates:
                if not task.accepts_lid(lid):
                    continue
                if active_counts[task.name] >= task.max_active:
                    continue
                active_counts[task.name] += 1
                queue.append((task, lid))
                break
        return queue

    def _random_padding(self) -> float:
        if hasattr(self.randomizer, "adi_waiting_time"):
            return float(self.randomizer.adi_waiting_time())
        return random.uniform(3.0, 8.0)


def bot_task(
    name: str,
    *,
    kingdom: Kingdom | int,
    attack: Attack,
    commanders: Sequence[int] | CommanderPool,
    target_level: int | None = None,
    target_levels: Sequence[int] = (),
    allocation: AllocationMode = AllocationMode.GREEDY,
    max_active: int | None = None,
    priority: int = 100,
    enabled: bool = True,
    tags: tuple[str, ...] = (),
    notes: str = "",
) -> Task:
    lids = tuple(int(value) for value in commanders)
    return Task(
        name=name,
        kingdom=kingdom,
        target_level=target_level,
        target_levels=tuple(int(level) for level in target_levels),
        commander_lids=lids,
        allocation=allocation,
        max_active=len(lids) if max_active is None else int(max_active),
        priority=priority,
        enabled=enabled,
        attack=attack,
        tags=tags,
        notes=notes,
    )


def task_definition(
    name: str,
    *,
    kingdom: Kingdom | int,
    attack: Attack,
    commanders: int,
    target_level: int | None = None,
    target_levels: Sequence[int] = (),
    allocation: AllocationMode = AllocationMode.GREEDY,
    max_active: int | None = None,
    priority: int = 100,
    enabled: bool = True,
    tags: tuple[str, ...] = (),
    notes: str = "",
) -> TaskDefinition:
    return TaskDefinition(
        name=name,
        kingdom=kingdom,
        attack=attack,
        commander_count=int(commanders),
        target_level=target_level,
        target_levels=tuple(int(level) for level in target_levels),
        allocation=allocation,
        max_active=max_active,
        priority=priority,
        enabled=enabled,
        tags=tags,
        notes=notes,
    )


def allocate_task_definitions(
    definitions: Sequence[TaskDefinition],
    *,
    first_commander: int = 1,
) -> list[Task]:
    tasks: list[Task] = []
    next_human_commander = first_commander
    for definition in definitions:
        if definition.commander_count < 1:
            raise ValueError(f"{definition.name} commander_count must be >= 1")
        last_human_commander = next_human_commander + definition.commander_count - 1
        lids = commander_range(next_human_commander, last_human_commander)
        tasks.append(
            bot_task(
                definition.name,
                kingdom=definition.kingdom,
                target_level=definition.target_level,
                target_levels=definition.target_levels,
                commanders=lids,
                allocation=definition.allocation,
                max_active=definition.commander_count if definition.max_active is None else definition.max_active,
                priority=definition.priority,
                enabled=definition.enabled,
                attack=definition.attack,
                tags=definition.tags,
                notes=definition.notes,
            )
        )
        next_human_commander = last_human_commander + 1
    return tasks


SAND_LEVEL_61_RBC_ATTACK = Attack(
    wave1=wave(
        left=side(
            units=[(TROOP.crossbowman, 50)],
        )
    )
)

MEAD_FLANK_ATTACK = Attack(
    wave1=wave(
        left = side(
            units = [(216, 30)],
            tools = [(TOOL.scaling_ladder, 5)]
        ),
        right = side(
            units = [(216, 30)],
            tools = [(TOOL.scaling_ladder, 5)]
        )
    ),
    wave2 = wave(
        left = side(
            units = [(216, 30)],
            tools = [(TOOL.scaling_ladder, 5)]
        ),
        right = side(
            units = [(216, 30)],
            tools = [(TOOL.scaling_ladder, 5)]
        )
    ),
    wave3 = wave(
        left = side(
            units = [(216, 30)],
            tools = [(TOOL.scaling_ladder, 5)]
        ),
        right = side(
            units = [(216, 30)],
            tools = [(TOOL.scaling_ladder, 5)]
        )
    ),
    wave4 = wave(
        left = side(
            units = [(216, 30)],
            tools = [(TOOL.scaling_ladder, 5)]
        ),
        right = side(
            units = [(216, 30)],
            tools = [(TOOL.scaling_ladder, 5)]
        )
    )
)

# STORM_

# Placeholder example. Replace the payload and enable it when you have the
# storm task details. Keeping it explicit prevents accidental empty sends.
STORM_CUSTOM_ATTACK = Attack(
    wave1=wave(
        left=side(
            units=[(TROOP.demon_horror, 50)],
        ),
        right= side(
            units=[(TROOP.demon_horror, 50)],
        ),
    ),
    wave2=wave(
        left=side(
            units=[(TROOP.demon_horror, 50)],
        ),
        right= side(
            units=[(TROOP.demon_horror, 50)],
        ),
    ),
    wave3=wave(
        left=side(
            units=[(TROOP.demon_horror, 50)],
        ),
        right= side(
            units=[(TROOP.demon_horror, 50)],
        ),
    ),
    wave4=wave(
        left=side(
            units=[(TROOP.demon_horror, 50)],
        ),
        right= side(
            units=[(TROOP.demon_horror, 50)],
        ),
    ),
)


TASK_DEFINITIONS: tuple[TaskDefinition, ...] = (
    task_definition(
        "sand_rbc_level_61_crossbow",
        kingdom=KINGDOM.sand,
        target_level=61,
        commanders=15,
        priority=10,
        attack=SAND_LEVEL_61_RBC_ATTACK,
        tags=("rbc", "sand"),
        notes="50 crossbowmen on left flank. Uses live ADI/CRA/GAM/CAT state before sending.",
    ),
    task_definition(
        "sand_rbc_level_36_60_mead_flank",
        kingdom=KINGDOM.sand,
        target_levels=tuple(range(36, 61)),
        commanders=20,
        priority=20,
        enabled=True,
        attack=MEAD_FLANK_ATTACK,
        tags=("rbc", "sand", "mead"),
        notes="Placeholder mead flank attack for Sands RBC levels 36-60 using commanders 14-35.",
    ),
    # task_definition(
    #     "storm_custom",
    #     kingdom=KINGDOM.storm,
    #     commanders=14,
    #     priority=20,
    #     enabled=True,
    #     attack=STORM_CUSTOM_ATTACK,
    #     tags=("storm",),
    #     notes="Storm event target task. Adjust STORM_CUSTOM_ATTACK before running if needed.",
    # ),
)


TASKS: list[Task] = allocate_task_definitions(TASK_DEFINITIONS)


DEFAULT_SCHEDULER = Scheduler(
    tasks=TASKS,
    kingdom_order=(KINGDOM.sand, KINGDOM.storm),
    kingdom_timeout=300.0,
    current_kingdom=KINGDOM.sand,
)


def create_bot(
    *,
    kingdom_timeout: float = 300.0,
    tasks: Sequence[Task] | None = None,
    definitions: Sequence[TaskDefinition] | None = None,
    kingdom_order: Sequence[Kingdom | int] = (KINGDOM.sand, KINGDOM.storm),
    starting_kingdom: Kingdom | int = KINGDOM.sand,
    first_commander: int = 1,
) -> Scheduler:
    if tasks is None:
        tasks = allocate_task_definitions(
            TASK_DEFINITIONS if definitions is None else definitions,
            first_commander=first_commander,
        )
    return Scheduler(
        tasks=list(tasks),
        kingdom_order=tuple(kingdom_order),
        kingdom_timeout=float(kingdom_timeout),
        current_kingdom=starting_kingdom,
    )


def task_summary(tasks: Iterable[Task] = TASKS) -> str:
    lines = []
    for task in tasks:
        if task.target_levels:
            if task.target_levels == tuple(range(min(task.target_levels), max(task.target_levels) + 1)):
                levels = f"{min(task.target_levels)}-{max(task.target_levels)}"
            else:
                levels = ",".join(str(level) for level in task.target_levels)
        else:
            levels = str(task.target_level)
        lines.append(
            f"{task.name}: enabled={task.enabled} kid={task.kingdom_id} "
            f"levels={levels} lids={list(task.commander_lids)} "
            f"max_active={task.max_active} priority={task.priority}"
        )
    return "\n".join(lines)


def plan_summary(scheduler: Scheduler = DEFAULT_SCHEDULER) -> str:
    plan = scheduler.current_plan()
    task_names = ", ".join(task.name for task in plan.tasks) or "none"
    return (
        f"current_kid={plan.kingdom_id} timeout={plan.timeout_seconds:.1f}s "
        f"tasks=[{task_names}]"
    )


if __name__ == "__main__":
    print(task_summary())
    print(plan_summary())
