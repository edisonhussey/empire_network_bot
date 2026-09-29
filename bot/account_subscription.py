"""Typed, serializable account-to-task subscription primitives."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Iterable

from .accounts import Account, normalize_account_name
from .game_data import Attack
from .scheduler import TaskDefinition


@dataclass(frozen=True)
class AccountSubscription:
    """The ordered task definitions assigned to one account."""

    account: Account
    tasks: tuple[TaskDefinition, ...]

    @property
    def key(self) -> str:
        return normalize_account_name(self.account.key)

    def to_public_dict(self) -> dict[str, object]:
        return {
            "account": self.account.to_public_dict(),
            "tasks": [task.name for task in self.tasks],
        }


def _validated_tasks(account: Account, tasks: Iterable[TaskDefinition]) -> tuple[TaskDefinition, ...]:
    result = tuple(tasks)
    for task in result:
        if isinstance(task, TaskDefinition):
            continue
        if isinstance(task, Attack):
            raise TypeError(
                f"account {account.key!r} subscribes to an Attack, not a task definition"
            )
        raise TypeError(
            f"account {account.key!r} subscribes to {type(task).__name__}; "
            "expected task_definition(...) objects"
        )
    names = [task.name for task in result]
    duplicates = sorted({name for name in names if names.count(name) > 1})
    if duplicates:
        raise ValueError(f"account {account.key!r} subscribes twice to: {duplicates}")
    return result


class _SubscriptionBuilder:
    def __init__(self, account: Account) -> None:
        if not isinstance(account, Account):
            raise TypeError(f"subscribe(...) requires an Account, got {type(account).__name__}")
        self.account = account

    def __call__(self, *tasks: TaskDefinition) -> AccountSubscription:
        return AccountSubscription(self.account, _validated_tasks(self.account, tasks))


def subscribe(account: Account) -> _SubscriptionBuilder:
    """Declare an ordered subscription: ``subscribe(Ventrilo)(task1, task2)``."""

    return _SubscriptionBuilder(account)


def subscription_index(
    subscriptions: Iterable[AccountSubscription],
) -> dict[str, tuple[TaskDefinition, ...]]:
    result: dict[str, tuple[TaskDefinition, ...]] = {}
    for subscription in subscriptions:
        if subscription.key in result:
            raise ValueError(f"duplicate subscription for account {subscription.account.key!r}")
        result[subscription.key] = subscription.tasks
    return result


__all__ = ["AccountSubscription", "subscribe", "subscription_index"]
