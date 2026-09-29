"""Small, mode-safe helpers for the file-backed proxy transport.

Drivers decide *what* to do and publish one command in ``pending``.  The
mitmproxy addon only sends that command and records the response.  Keeping the
mode check here makes it testable without importing mitmproxy or ``bot.bot``.
"""

from __future__ import annotations

import time
from typing import Any


READ_ONLY_KINDS = frozenset({"gaa_probe", "fnt_probe"})


def sands_adi_timeout_age(
    pending: dict[str, Any] | None,
    *,
    timeout_seconds: float,
    now: float | None = None,
) -> float | None:
    """Age of a sent Sands ADI once it has exceeded its response deadline."""

    if not isinstance(pending, dict) or pending.get("kind") != "adi":
        return None
    try:
        sent_at = float(pending.get("sent_at"))
    except (TypeError, ValueError):
        return None
    age = (time.time() if now is None else float(now)) - sent_at
    return age if age > float(timeout_seconds) else None


def record_sands_adi_timeout_error(
    state: dict[str, Any],
    *,
    now: int,
    tolerated_errors: int,
) -> tuple[int, list[int], bool]:
    """Record one timeout and report whether its consecutive budget is spent."""

    hourly_errors = [
        int(value)
        for value in state.get("cra_error_timestamps", [])
        if isinstance(value, (int, float)) and now - int(value) < 3600
    ]
    hourly_errors.append(int(now))
    consecutive_errors = int(state.get("cra_consecutive_errors", 0) or 0) + 1
    state["cra_consecutive_errors"] = consecutive_errors
    state["cra_error_timestamps"] = hourly_errors
    return consecutive_errors, hourly_errors, consecutive_errors > int(tolerated_errors)


def command_mode(command: dict[str, Any] | None) -> str | None:
    """Return the only mode allowed to execute *command*.

    Read-only map probes are mode-neutral.  Every state-changing command is
    deliberately fail-closed: an unknown shape has no owner and cannot send.
    """

    if not isinstance(command, dict):
        return None
    kind = command.get("kind")
    target_kind = command.get("target_kind")
    if kind in READ_ONLY_KINDS:
        return "any"
    if kind == "kut_transfer":
        return "berimond"
    if kind == "msk_skip":
        return "berimond"
    if target_kind == "berimond_fixed" and kind in {"aci", "cra"}:
        return "berimond"
    if target_kind == "storm_target" and kind in {"adi", "cra"}:
        return "storm"
    if kind in {"adi", "cra"} and target_kind in {None, "rbc"}:
        return "sands"
    return None


def command_allowed(state: dict[str, Any], command: dict[str, Any] | None) -> bool:
    """Whether the current run explicitly owns *command*.

    This prevents a stale Sands CRA from being sent after a Berimond driver has
    taken ownership of the same control file.  It also means ``running=true``
    without an explicit mode can never send an attack.
    """

    owner = command_mode(command)
    return owner == "any" or (bool(state.get("running")) and owner == state.get("mode"))


def command_id(command: dict[str, Any] | None) -> str | None:
    if not isinstance(command, dict):
        return None
    value = command.get("command_id")
    return str(value) if value is not None else None


def response_matches(command: dict[str, Any], response: dict[str, Any] | None) -> bool:
    """Match a transport response to the command that caused it."""

    expected = command_id(command)
    return bool(expected and isinstance(response, dict) and command_id(response) == expected)


def berimond_attack_config(
    command: dict[str, Any] | None,
    state: dict[str, Any],
) -> list[dict[str, Any]] | None:
    """Return the controller-supplied army, preferring the command snapshot.

    mitmproxy is intentionally long-lived and therefore has stale imported
    Python modules after ``bot/event/berimond_kingdom/config.py`` is edited. The short-lived
    Berimond controller serializes its current ``ATTACK`` into both the run
    state and every ACI command. The command wins so an in-flight handshake
    cannot silently change army halfway through.
    """

    candidates = (
        command.get("berimond_attack") if isinstance(command, dict) else None,
        state.get("berimond_attack"),
    )
    for candidate in candidates:
        if isinstance(candidate, list) and all(isinstance(wave, dict) for wave in candidate):
            return candidate
    return None
