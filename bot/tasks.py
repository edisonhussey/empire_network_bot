"""Task queue definitions — what the bot farms, with which commanders.

This is the editable "plan" layer. Each entry describes one repeatable job:
which kingdom it runs in, which RBC levels it accepts, how many commanders it
may consume and which :mod:`bot.attacks` payload it sends.

The scheduling *engine* (allocation, priority, kingdom rotation) lives in
:mod:`bot.scheduler`; only the data lives here so it can be edited without
touching engine code.

Commander numbers are 1-based human numbers and are mapped to server LIDs by
:func:`bot.scheduler.commander_lid`. They are handed out to tasks in order, so
task order decides who gets which commanders.
"""

from __future__ import annotations

from typing import Sequence

from .accounts import normalize_account_name
from .attacks import (
    ATTACK_REGISTRY,
    SANDS_35_61_DEATHLY_HORROR,
    SANDS_LV36_60_MEAD,
    SANDS_LV61,
)
from .game_data import KINGDOM, Attack
from .scheduler import (
    Scheduler,
    Task,
    TaskDefinition,
    allocate_task_definitions,
    task_definition,
)

# ---------------------------------------------------------------------------
# Task catalogue - each task is a standalone object
# ---------------------------------------------------------------------------
#
# Write a task once, name it, and let accounts subscribe to it by name.
# To vary one detail for one account, add another standalone task here rather
# than mutating an existing one - then the object stays obvious and typed.

SAND_LV61_CROSSBOW = task_definition(
    "sand_rbc_level_61_crossbow",
    kingdom=KINGDOM.sand,
    target_level=61,
    commanders=16,
    priority=20,
    attack=SANDS_LV61,
    tags=("rbc", "sand"),
    notes="50 crossbowmen on left flank. Uses live ADI/CRA/GAM/CAT state before sending.",
)

#: Same raid, more commanders and higher priority. A separate object so
#: `sand_rbc_level_61_crossbow` stays exactly as it is for other accounts.
SAND_LV61_CROSSBOW_20C = task_definition(
    "sand_rbc_level_61_crossbow_20c",
    kingdom=KINGDOM.sand,
    target_level=61,
    commanders=20,
    priority=30,
    attack=SANDS_LV61,
    tags=("rbc", "sand"),
    notes="As sand_rbc_level_61_crossbow but 20 commanders at higher priority.",
)

SAND_LV36_60_MEAD_FLANK = task_definition(
    "sand_rbc_level_36_60_mead_flank",
    kingdom=KINGDOM.sand,
    target_levels=tuple(range(35, 61)),
    commanders=19,
    priority=10,
    enabled=True,
    attack=SANDS_LV36_60_MEAD,
    tags=("rbc", "sand", "mead"),
    notes="Mead flank attack for Sands RBC levels 36-60 using commanders 17-35.",
)

SAND_LV35_61_DEATHLY_HORROR = task_definition(
    "sand_lv35_61_dh",
    kingdom=KINGDOM.sand,
    target_levels=tuple(range(35, 61)),
    commanders=18,
    priority=10,
    enabled=True,
    attack=SANDS_35_61_DEATHLY_HORROR,
    tags=("rbc", "sand"),
    notes="Deathly horror clear for Sands RBC levels 35-60, 30 + 5 ladders per side.",
)

# Uncomment once the Storm payload and source coordinates are confirmed.
# `bot.attacks.STORM_DEMON_HORROR` holds the placeholder composition.
# STORM_CUSTOM = task_definition(
#     "storm_custom",
#     kingdom=KINGDOM.storm,
#     commanders=14,
#     priority=20,
#     enabled=True,
#     attack=STORM_DEMON_HORROR,
#     tags=("storm",),
#     notes="Storm event target task.",
# )

#: Every attack from :mod:`bot.attacks`, addressable by name, so a task can use
#: one without importing it: ``attack=ATTACKS["sands_35_61_deathly_horror"]``
ATTACKS = ATTACK_REGISTRY


#: Every task defined above, addressable by name.
#:
#: Collected automatically from the module-level ``task_definition(...)``
#: objects, so a new task needs no dict edit. Duplicate names are a real bug
#: (a later one would silently win), so they raise instead.
def _collect_tasks() -> dict[str, TaskDefinition]:
    found: dict[str, TaskDefinition] = {}
    for value in list(globals().values()):
        if not isinstance(value, TaskDefinition):
            continue
        if value.name in found:
            raise ValueError(f"duplicate task name {value.name!r} in bot/tasks.py")
        found[value.name] = value
    return found


TASKS_BY_NAME: dict[str, TaskDefinition] = _collect_tasks()

#: Kingdom rotation order for the scheduler.
KINGDOM_ORDER = (KINGDOM.sand, KINGDOM.storm)

#: Seconds spent in one kingdom before rotating.
KINGDOM_TIMEOUT_SECONDS = 300.0


# ---------------------------------------------------------------------------
# Subscriptions - which tasks an account runs, and in what order
# ---------------------------------------------------------------------------
#
# Subscriptions reference the task OBJECTS above, not names. That way:
#   * a wrong reference is an immediate NameError, not a runtime surprise
#   * your editor can jump to the definition and autocomplete it
#   * nothing has to be kept in sync by hand
#
# Commanders are handed out in the order they appear, so the order below decides
# who gets which commanders.
#
#   * no entry for an account -> it uses DEFAULT_TASKS, so a brand new account
#     needs no edits at all
#   * an entry -> exactly those tasks, in that order

#: The task list every account runs unless it subscribes to its own.
DEFAULT_TASKS: tuple[TaskDefinition, ...] = (
    SAND_LV61_CROSSBOW,
    SAND_LV36_60_MEAD_FLANK,
)

#: Per-account subscriptions.
ACCOUNT_TASKS: dict[str, tuple[TaskDefinition, ...]] = {
    "ventrilo": (
        SAND_LV61_CROSSBOW,
        SAND_LV36_60_MEAD_FLANK,
    ),
    "pingpoko": (
        SAND_LV35_61_DEATHLY_HORROR,
    ),
}


# ---------------------------------------------------------------------------
# Derived plans
# ---------------------------------------------------------------------------


#: Definitions from the default subscription, in order.
DEFAULT_TASK_DEFINITIONS: tuple[TaskDefinition, ...] = DEFAULT_TASKS


def task_definitions_for(account_name: str | None = None) -> tuple[TaskDefinition, ...]:
    """The subscribed definitions for an account, in the order listed.

    Order matters: commanders are allocated in list order, so the first entry in
    the subscription gets the first commander numbers.
    """

    key = normalize_account_name(account_name or "")
    chosen = ACCOUNT_TASKS[key] if key and key in ACCOUNT_TASKS else DEFAULT_TASKS

    # Subscriptions must hold task objects. The easy mistake is reaching for an
    # Attack instead - the names look alike (SANDS_* attack, SAND_* task) - so
    # say that plainly rather than failing on a missing attribute later.
    for entry in chosen:
        if isinstance(entry, TaskDefinition):
            continue
        if isinstance(entry, Attack):
            raise TypeError(
                f"account {key or '<default>'!r} subscribes to an Attack, not a task. "
                "Use the task object from this module - attack objects live in bot/attacks.py."
            )
        raise TypeError(
            f"account {key or '<default>'!r} subscribes to a {type(entry).__name__}; "
            "expected a task_definition(...) object"
        )

    # Listing the same task twice would allocate its commanders twice and give
    # the scheduler two tasks sharing a name - always a mistake.
    names = [definition.name for definition in chosen]
    duplicates = sorted({name for name in names if names.count(name) > 1})
    if duplicates:
        raise ValueError(
            f"account {key or '<default>'!r} subscribes to the same task twice: {duplicates}"
        )
    return tuple(chosen)


def task_names_for(account_name: str | None = None) -> tuple[str, ...]:
    """The task names an account subscribes to, in allocation order."""

    return tuple(definition.name for definition in task_definitions_for(account_name))


def tasks_for(account_name: str | None = None, *, first_commander: int = 1) -> list[Task]:
    """The allocated task list for an account (commander numbers handed out in order)."""

    return allocate_task_definitions(task_definitions_for(account_name), first_commander=first_commander)


def scheduler_for(account_name: str | None = None, **kwargs) -> Scheduler:
    """A scheduler built from an account's plan."""

    return create_bot(account_name=account_name, **kwargs)


def set_active_account(account_name: str | None) -> list[Task]:
    """Rebind the module-level plan to an account.

    Called from ``bot.bot.configure_account`` so every existing runner picks up
    the right plan without threading an account through the call chain.
    """

    global TASK_DEFINITIONS, TASKS, DEFAULT_SCHEDULER

    TASK_DEFINITIONS = task_definitions_for(account_name)
    TASKS = allocate_task_definitions(TASK_DEFINITIONS)
    DEFAULT_SCHEDULER = Scheduler(
        tasks=TASKS,
        kingdom_order=KINGDOM_ORDER,
        kingdom_timeout=KINGDOM_TIMEOUT_SECONDS,
        current_kingdom=KINGDOM.sand,
    )
    return TASKS


def build_tasks(definitions: Sequence[TaskDefinition] | None = None) -> list[Task]:
    """Allocate commander numbers to task definitions."""

    return allocate_task_definitions(TASK_DEFINITIONS if definitions is None else definitions)


TASK_DEFINITIONS: tuple[TaskDefinition, ...] = DEFAULT_TASK_DEFINITIONS
TASKS: list[Task] = build_tasks()


def create_bot(
    *,
    account_name: str | None = None,
    kingdom_timeout: float = KINGDOM_TIMEOUT_SECONDS,
    tasks: Sequence[Task] | None = None,
    definitions: Sequence[TaskDefinition] | None = None,
    kingdom_order: Sequence = KINGDOM_ORDER,
    starting_kingdom=KINGDOM.sand,
    first_commander: int = 1,
) -> Scheduler:
    """Build a fresh :class:`~bot.scheduler.Scheduler` from a task plan."""

    if tasks is None:
        chosen = definitions if definitions is not None else task_definitions_for(account_name)
        tasks = allocate_task_definitions(chosen, first_commander=first_commander)
    return Scheduler(
        tasks=list(tasks),
        kingdom_order=tuple(kingdom_order),
        kingdom_timeout=float(kingdom_timeout),
        current_kingdom=starting_kingdom,
    )


DEFAULT_SCHEDULER = Scheduler(
    tasks=TASKS,
    kingdom_order=KINGDOM_ORDER,
    kingdom_timeout=KINGDOM_TIMEOUT_SECONDS,
    current_kingdom=KINGDOM.sand,
)


def task_summary(tasks: Sequence[Task] | None = None) -> str:
    """One line per task, for the CLI."""

    lines = []
    for task in TASKS if tasks is None else tasks:
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
    return f"current_kid={plan.kingdom_id} timeout={plan.timeout_seconds:.1f}s tasks=[{task_names}]"


def enabled_task_names() -> list[str]:
    return [task.name for task in TASKS if task.enabled]


__all__ = [
    "ACCOUNT_TASKS",
    "ATTACKS",
    "DEFAULT_SCHEDULER",
    "DEFAULT_TASK_DEFINITIONS",
    "DEFAULT_TASKS",
    "KINGDOM_ORDER",
    "KINGDOM_TIMEOUT_SECONDS",
    "SAND_LV35_61_DEATHLY_HORROR",
    "SAND_LV36_60_MEAD_FLANK",
    "SAND_LV61_CROSSBOW",
    "SAND_LV61_CROSSBOW_20C",
    "TASKS",
    "TASKS_BY_NAME",
    "TASK_DEFINITIONS",
    "build_tasks",
    "create_bot",
    "enabled_task_names",
    "plan_summary",
    "scheduler_for",
    "set_active_account",
    "task_definitions_for",
    "task_names_for",
    "task_summary",
    "tasks_for",
]
