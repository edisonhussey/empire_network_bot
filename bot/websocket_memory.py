"""Bound mitmproxy's otherwise unbounded per-flow WebSocket history."""

from __future__ import annotations

from typing import Any


# The listener processes messages synchronously and only reads the newest one.
# Retaining a few entries is useful for diagnostics without keeping a complete
# multi-hour game session (which can grow to several gigabytes in RAM).
WEBSOCKET_HISTORY_LIMIT = 8


def trim_websocket_history(websocket: Any, *, limit: int = WEBSOCKET_HISTORY_LIMIT) -> int:
    """Discard old messages in-place and return the number released."""

    if limit < 1:
        raise ValueError("websocket history limit must be at least one")
    messages = getattr(websocket, "messages", None)
    if not isinstance(messages, list):
        return 0
    excess = len(messages) - limit
    if excess <= 0:
        return 0
    del messages[:excess]
    return excess
