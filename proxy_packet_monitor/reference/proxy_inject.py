"""Queue one-off packets for the already-running mitmproxy listener.

This is deliberately a tiny file-backed bridge: the caller and mitmproxy live
in different Python processes, so importing ``rbc_proxy_listener.inject_packet``
would create a second, disconnected copy of the addon.  A request file is
written atomically into the logged-in account's directory; the live addon
claims it and sends it through its normal guarded injection path.

Typical interactive use::

    from bot.proxy_inject import inject
    inject("%xt%EmpireEx_21%gaa%1%{\"KID\":1,...}%")

The function waits briefly for the listener's accepted/rejected result.  It
does not wait for the game's reply; that still appears in the normal capture.
"""

from __future__ import annotations

import json
import time
import uuid
from pathlib import Path
from typing import Any

from . import accounts
from .account_context import AccountContext
from .packets import SAND_SERVER_HEADER, parse_xt_packet


INBOX_NAME = "proxy_injections"
DEFAULT_EXPIRES_SECONDS = 30.0
DEFAULT_WAIT_SECONDS = 5.0


def injection_dir(context: AccountContext) -> Path:
    path = context.root / INBOX_NAME
    path.mkdir(parents=True, exist_ok=True)
    return path


def _context(account: str | None) -> AccountContext:
    selected = accounts.load_account(account) if account else accounts.session_account()
    if selected is None:
        raise RuntimeError("no detected login session; pass account='name'")
    return selected.context


def build_packet(
    command: str,
    payload: Any,
    *,
    request_id: int = 1,
    server_header: str = SAND_SERVER_HEADER,
) -> str:
    """Build a client XT frame for an ad-hoc command and payload."""

    command = str(command).strip()
    if not command or "%" in command:
        raise ValueError("command must be a non-empty XT command name")
    return "%xt%{}%{}%{}%{}%".format(
        server_header,
        command,
        int(request_id),
        json.dumps(payload, separators=(",", ":")),
    )


def _packet(message: str, payload: Any | None, request_id: int, server_header: str) -> str:
    if payload is not None:
        return build_packet(message, payload, request_id=request_id, server_header=server_header)
    packet = str(message).strip()
    if parse_xt_packet(packet) is None:
        raise ValueError("message is not a valid XT packet; pass payload=... to build one from a command")
    return packet


def inject(
    message: str,
    payload: Any | None = None,
    *,
    account: str | None = None,
    request_id: int = 1,
    server_header: str = SAND_SERVER_HEADER,
    expires_seconds: float = DEFAULT_EXPIRES_SECONDS,
    wait_seconds: float = DEFAULT_WAIT_SECONDS,
) -> dict[str, Any]:
    """Queue a custom packet and return the listener's immediate result.

    ``message`` may be a complete raw XT packet.  When ``payload`` is supplied,
    ``message`` is treated as the command name and a packet is built for it.
    A returned ``status='sent'`` means mitmproxy injected the packet; the game
    server's response is asynchronous and is recorded by the normal capture.
    """

    context = _context(account)
    packet = _packet(message, payload, request_id, server_header)
    parsed = parse_xt_packet(packet)
    assert parsed is not None

    now = time.time()
    request_token = uuid.uuid4().hex
    folder = injection_dir(context)
    request_path = folder / f"{request_token}.request.json"
    result_path = folder / f"{request_token}.result.json"
    body = {
        "id": request_token,
        "account": context.aid,
        "command": parsed.get("command"),
        "packet": packet,
        "created_at": now,
        "expires_at": now + max(1.0, float(expires_seconds)),
    }
    temporary = folder / f".{request_token}.tmp"
    temporary.write_text(json.dumps(body, separators=(",", ":")) + "\n", encoding="utf-8")
    temporary.replace(request_path)

    deadline = time.monotonic() + max(0.0, float(wait_seconds))
    while time.monotonic() < deadline:
        if result_path.exists():
            try:
                result = json.loads(result_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                time.sleep(0.05)
                continue
            try:
                result_path.unlink()
            except FileNotFoundError:
                pass
            return result
        time.sleep(0.05)

    return {
        "id": request_token,
        "account": context.aid,
        "command": parsed.get("command"),
        "status": "queued",
        "request_path": str(request_path),
    }


def claim_next(context: AccountContext) -> tuple[Path, dict[str, Any]] | None:
    """Atomically claim the oldest request. Used only by the live addon."""

    folder = injection_dir(context)
    # A listener restart may leave a request claimed but unfinished. There is
    # only one addon consumer, so reclaiming it here is safe and prevents loss.
    processing = sorted(folder.glob("*.processing.json"), key=lambda path: path.stat().st_mtime)
    if processing:
        claimed_path = processing[0]
        try:
            body = json.loads(claimed_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            body = {"id": claimed_path.name.split(".", 1)[0], "invalid": True}
        return claimed_path, body
    for request_path in sorted(folder.glob("*.request.json"), key=lambda path: path.stat().st_mtime):
        claimed_path = request_path.with_name(request_path.name.replace(".request.json", ".processing.json"))
        try:
            request_path.replace(claimed_path)
        except FileNotFoundError:
            continue
        try:
            body = json.loads(claimed_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            body = {"id": claimed_path.name.split(".", 1)[0], "invalid": True}
        return claimed_path, body
    return None


def requeue(claimed_path: Path) -> None:
    request_path = claimed_path.with_name(claimed_path.name.replace(".processing.json", ".request.json"))
    try:
        claimed_path.replace(request_path)
    except FileNotFoundError:
        pass


def finish(context: AccountContext, claimed_path: Path, request: dict[str, Any], **result: Any) -> None:
    """Publish a result atomically and remove the claimed request."""

    request_token = str(request.get("id") or claimed_path.name.split(".", 1)[0])
    body = {
        "id": request_token,
        "account": context.aid,
        "command": request.get("command"),
        "finished_at": time.time(),
        **result,
    }
    folder = injection_dir(context)
    temporary = folder / f".{request_token}.result.tmp"
    result_path = folder / f"{request_token}.result.json"
    temporary.write_text(json.dumps(body, separators=(",", ":")) + "\n", encoding="utf-8")
    temporary.replace(result_path)
    try:
        claimed_path.unlink()
    except FileNotFoundError:
        pass
