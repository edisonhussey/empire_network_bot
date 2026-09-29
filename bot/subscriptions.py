"""The single account-to-task subscription document.

Account objects are discovered from ``credentials/*.env`` or matching folders
under ``bot/account_data``. Task order matters because commander numbers are
allocated in this order.

This module is intentionally plain declarative data. A future website can read
``subscription_records()`` without understanding the scheduler internals.
"""

from __future__ import annotations

from .account_subscription import AccountSubscription, subscribe, subscription_index
from .accounts import Account, Pingpoko, Ventrilo, normalize_account_name
from .event.sand.config import (
    SAND_KUNAI,
    SAND_LV35_61_DEATHLY_HORROR,
    SAND_LV36_60_MEAD_FLANK,
    SAND_LV61_CROSSBOW,
)
from .scheduler import TaskDefinition


DEFAULT_TASKS: tuple[TaskDefinition, ...] = (
    SAND_LV61_CROSSBOW,
    SAND_LV36_60_MEAD_FLANK,
)

SUBSCRIPTIONS: tuple[AccountSubscription, ...] = (
    # subscribe(Ventrilo)(
    #     SAND_LV61_CROSSBOW,
    #     SAND_LV36_60_MEAD_FLANK,
    # ),
    subscribe(Ventrilo)(
        SAND_LV61_CROSSBOW,
        SAND_KUNAI
    ),
    subscribe(Pingpoko)(
        SAND_LV35_61_DEATHLY_HORROR,
    ),
)

# Compatibility/index view consumed by the scheduler.
ACCOUNT_TASKS: dict[str, tuple[TaskDefinition, ...]] = subscription_index(SUBSCRIPTIONS)


def subscription_for(account: Account | str) -> AccountSubscription | None:
    """Return one declaration by account object or web/CLI account slug."""

    key = account.slug if isinstance(account, Account) else normalize_account_name(account)
    return next((item for item in SUBSCRIPTIONS if item.key == key), None)


def subscription_records() -> tuple[dict[str, object], ...]:
    return tuple(subscription.to_public_dict() for subscription in SUBSCRIPTIONS)


__all__ = [
    "ACCOUNT_TASKS",
    "DEFAULT_TASKS",
    "SUBSCRIPTIONS",
    "subscribe",
    "subscription_for",
    "subscription_records",
]
