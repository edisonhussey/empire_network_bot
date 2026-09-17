from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .aid_translation import resolve_aid  # noqa: F401  (kept for callers/tests)


REPO_ROOT = Path(__file__).resolve().parents[1]
BOT_DIR = REPO_ROOT / "bot"


@dataclass(frozen=True)
class AccountContext:
    """Runtime directories for one account.

    ``aid`` is the account's canonical scope key. It used to be a numeric game
    account id, but the login handshake's ``AID`` turned out to be a **shared
    portal id** identical across accounts, so the key is now the login name.
    """

    username: str
    aid: str
    root: Path
    logs_dir: Path
    latest_logs_dir: Path
    gamestate_dir: Path
    control_file: Path
    listener_log: Path


def for_account_name(account_name: str) -> AccountContext:
    return for_key(account_name, account_name)


def for_key(account_key: str, username: str | None = None) -> AccountContext:
    """Locate an account's runtime directories by its canonical key."""

    account_root = BOT_DIR / "account_data" / str(account_key)
    context = AccountContext(
        username=username or "",
        aid=str(account_key),
        root=account_root,
        logs_dir=account_root / "logs",
        latest_logs_dir=account_root / "latest_logs",
        gamestate_dir=account_root / "gamestate",
        control_file=account_root / "proxy_control.json",
        listener_log=account_root / "rbc_proxy_listener.log",
    )
    ensure_dirs(context)
    return context


def for_aid(aid: str, username: str | None = None) -> AccountContext:
    """Legacy alias for :func:`for_key`."""

    return for_key(aid, username)


def ensure_dirs(context: AccountContext) -> None:
    for path in (context.root, context.logs_dir, context.latest_logs_dir, context.gamestate_dir):
        path.mkdir(parents=True, exist_ok=True)
    for name in ("commander_state.json", "rbc_state.json"):
        path = context.gamestate_dir / name
        if not path.exists():
            path.write_text("{}\n", encoding="utf-8")
