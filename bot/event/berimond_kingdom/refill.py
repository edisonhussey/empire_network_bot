"""Pure Berimond camp-stock and refill calculations."""

from __future__ import annotations

import time
from typing import Any, Callable

from . import config
from ...packets import army_requirements


def army_need() -> dict[int, int]:
    return dict(army_requirements(config.ATTACK.to_payload()))


def camp_stock(state: dict[str, Any]) -> tuple[dict[int, int], int]:
    raw = state.get("berimond_stock")
    if not isinstance(raw, dict):
        return {}, 0
    inventory: dict[int, int] = {}
    for unit_id, count in (raw.get("inventory") or {}).items():
        try:
            inventory[int(unit_id)] = int(count)
        except (TypeError, ValueError):
            continue
    return inventory, int(raw.get("at", 0) or 0)


def camp_transit(state: dict[str, Any]) -> dict[int, int]:
    """Units counted by the camp headquarters under ``gui.TU``."""

    raw = state.get("berimond_stock")
    if not isinstance(raw, dict):
        return {}
    transit: dict[int, int] = {}
    for unit_id, count in (raw.get("transit") or {}).items():
        try:
            transit[int(unit_id)] = int(count)
        except (TypeError, ValueError):
            continue
    return transit


def headquarters_occupancy(state: dict[str, Any]) -> dict[int, int]:
    home, _ = camp_stock(state)
    transit = camp_transit(state)
    return {
        unit_id: home.get(unit_id, 0) + transit.get(unit_id, 0)
        for unit_id in set(home) | set(transit)
    }


def describe_camp_stock(state: dict[str, Any], log: Callable[[str], None]) -> None:
    inventory, captured = camp_stock(state)
    need = army_need()
    if not inventory:
        log("berimond_camp_stock unknown (no gui/aci stock captured yet)")
        return
    age = max(0, int(time.time()) - captured) if captured else -1
    have = " ".join(f"u{unit_id}={inventory.get(unit_id, 0)}" for unit_id in sorted(need))
    want = " ".join(f"u{unit_id}={count}" for unit_id, count in sorted(need.items()))
    log(f"berimond_camp_stock age={age}s need={want} have={have}")
    short = [unit_id for unit_id, count in need.items() if inventory.get(unit_id, 0) < count]
    if short:
        detail = " ".join(
            f"u{unit_id}={inventory.get(unit_id, 0)}/{need[unit_id]}" for unit_id in sorted(short)
        )
        log(f"berimond_stock_low {detail} -> refill the camp before the next attack")


def capacity_for(unit_id: int) -> int:
    overrides = getattr(config, "CAPACITY_BY_UNIT", None)
    if isinstance(overrides, dict) and overrides:
        return int(overrides.get(int(unit_id), 0) or 0)
    return int(getattr(config, "TOTAL_CAPACITY", 0) or 0)


def returned_units(item: dict[str, Any]) -> dict[int, int]:
    out: dict[int, int] = {}
    for unit_text, count in (item.get("units") or {}).items():
        try:
            out[int(unit_text)] = int(count)
        except (TypeError, ValueError):
            continue
    return out


def survival_ratios(state: dict[str, Any], sent: dict[int, int]) -> dict[int, float]:
    returns = [item for item in (state.get("berimond_returns") or []) if isinstance(item, dict)]
    if not returns:
        return {}
    ratios: dict[int, float] = {}
    for unit_id, per_attack in sent.items():
        returned = sum(returned_units(item).get(int(unit_id), 0) for item in returns)
        carried = int(per_attack) * len(returns)
        if carried > 0 and returned > 0:
            ratios[int(unit_id)] = min(1.0, returned / carried)
    return ratios


def projected_unit_counts(
    state: dict[str, Any],
    sent: dict[int, int],
    *,
    now: float | None = None,
) -> dict[int, dict[str, float]]:
    """Project the roster after all currently-known marches return."""

    home, captured = camp_stock(state)
    assigned = camp_transit(state)
    ratios = survival_ratios(state, sent)
    current_time = time.time() if now is None else float(now)
    returning: dict[int, int] = {}
    landed: dict[int, int] = {}
    for item in (state.get("berimond_returns") or []):
        if not isinstance(item, dict):
            continue
        arrives = float(item.get("arrives_at", 0) or 0)
        for unit_id, count in returned_units(item).items():
            if arrives > current_time:
                returning[unit_id] = returning.get(unit_id, 0) + count
            elif captured and arrives > captured:
                landed[unit_id] = landed.get(unit_id, 0) + count

    result: dict[int, dict[str, float]] = {}
    unit_ids = set(home) | set(assigned) | set(returning) | set(landed) | set(sent)
    for unit_id in unit_ids:
        survival = float(ratios.get(unit_id, 1.0))
        assigned_return = float(assigned.get(unit_id, 0)) * survival
        total = (
            float(home.get(unit_id, 0))
            + float(landed.get(unit_id, 0))
            + float(returning.get(unit_id, 0))
            + assigned_return
        )
        result[unit_id] = {
            "home": float(home.get(unit_id, 0)),
            "landed": float(landed.get(unit_id, 0)),
            "returning": float(returning.get(unit_id, 0)),
            "assigned": float(assigned.get(unit_id, 0)),
            "survival": survival,
            "assigned_return": assigned_return,
            "total": total,
        }
    return result


def plan_refill(
    *,
    sent: dict[int, int],
    expected: dict[int, float],
    safety: float = 0.7,
) -> list[list[int]]:
    plan: list[list[int]] = []
    bounded_safety = max(0.0, min(1.0, float(safety)))
    for unit_id in sorted(sent):
        capacity = capacity_for(unit_id)
        target = float(capacity) * bounded_safety
        amount = int(target - float(expected.get(int(unit_id), 0.0)))
        if capacity > 0 and amount > 0:
            plan.append([int(unit_id), amount])
    return plan
