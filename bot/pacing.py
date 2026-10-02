"""Packet pacing policies shared by the proxy transports.

Pacing has three separate concerns which must not share one timer:

* the hard minimum between CRA packets;
* normal, successful request/response timing; and
* error backoff after a failed operation.

The listener owns error backoff because the appropriate delay depends on the
failure.  This module owns the successful attack cadence and the invariant that
no CRA packet is injected less than four seconds after the preceding CRA.
"""

from __future__ import annotations

from dataclasses import dataclass
import random
from typing import Callable


Uniform = Callable[[float, float], float]


@dataclass(frozen=True)
class AttackPacingPolicy:
    """Timing policy for successful attack handshakes.

    Jitter is always added *above* ``cra_min_interval``.  The separate
    :meth:`hard_cra_due_at` check deliberately adds no jitter; it is the final
    safety check immediately before injection and cannot move a previously
    sampled deadline earlier.
    """

    cra_min_interval: float = 4.0
    cra_jitter_range: tuple[float, float] = (0.15, 0.65)
    sands_after_cra_ack_range: tuple[float, float] = (0.75, 2.25)
    sands_adi_to_cra_range: tuple[float, float] = (5.5, 9.0)

    @staticmethod
    def _sample(bounds: tuple[float, float], uniform: Uniform) -> float:
        lower, upper = (float(bounds[0]), float(bounds[1]))
        if lower < 0 or upper < lower:
            raise ValueError(f"invalid pacing range {bounds!r}")
        return float(uniform(lower, upper))

    def sands_after_cra_ack_delay(self, *, uniform: Uniform = random.uniform) -> float:
        return self._sample(self.sands_after_cra_ack_range, uniform)

    def sands_cra_due_at(
        self,
        *,
        now: float,
        last_cra_at: float | None,
        uniform: Uniform = random.uniform,
    ) -> float:
        """Return one sampled deadline for a verified Sands CRA.

        The ADI-to-CRA variance and the CRA-to-CRA floor are independent.  The
        later deadline wins, so shortening normal handshake delays can never
        weaken the global CRA invariant.
        """

        adi_due = float(now) + self._sample(self.sands_adi_to_cra_range, uniform)
        if last_cra_at is None or float(last_cra_at) <= 0:
            return adi_due
        cra_due = (
            float(last_cra_at)
            + float(self.cra_min_interval)
            + self._sample(self.cra_jitter_range, uniform)
        )
        return max(adi_due, cra_due)

    def hard_cra_due_at(self, last_cra_at: float | None) -> float:
        """Earliest legal CRA time, used as the final pre-injection guard."""

        if last_cra_at is None or float(last_cra_at) <= 0:
            return 0.0
        return float(last_cra_at) + float(self.cra_min_interval)

    def enforce_cra_deadline(self, requested_due_at: float, last_cra_at: float | None) -> float:
        return max(float(requested_due_at), self.hard_cra_due_at(last_cra_at))


DEFAULT_ATTACK_PACING = AttackPacingPolicy()


def state_last_cra_at(state: dict) -> float | None:
    """Read the newest persisted high-resolution CRA timestamp.

    ``last_global_attack_sent_at`` is retained as a compatibility fallback for
    control files written before the transport-wide timestamp was introduced.
    """

    values: list[float] = []
    for key in ("last_cra_injected_at", "last_global_attack_sent_at"):
        try:
            value = float(state.get(key, 0.0) or 0.0)
        except (TypeError, ValueError):
            continue
        if value > 0:
            values.append(value)
    return max(values) if values else None
