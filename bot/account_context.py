from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from bot.aid_translation import resolve_aid


REPO_ROOT = Path(__file__).resolve().parents[1]
BOT_DIR = REPO_ROOT / "bot"


@dataclass(frozen=True)
class AccountContext:
    username: str
    aid: str
    root: Path
    logs_dir: Path
    latest_logs_dir: Path
    gamestate_dir: Path
    control_file: Path
    listener_log: Path


def for_account_name(account_name: str) -> AccountContext:
    return for_aid(resolve_aid(account_name), account_name)


def for_aid(aid: str, username: str | None = None) -> AccountContext:
    account_root = BOT_DIR / "account_data" / str(aid)
    context = AccountContext(
        username=username or "",
        aid=str(aid),
        root=account_root,
        logs_dir=account_root / "logs",
        latest_logs_dir=account_root / "latest_logs",
        gamestate_dir=account_root / "gamestate",
        control_file=account_root / "proxy_control.json",
        listener_log=account_root / "rbc_proxy_listener.log",
    )
    ensure_dirs(context)
    return context


def ensure_dirs(context: AccountContext) -> None:
    for path in (context.root, context.logs_dir, context.latest_logs_dir, context.gamestate_dir):
        path.mkdir(parents=True, exist_ok=True)
    for name in ("commander_state.json", "rbc_state.json"):
        path = context.gamestate_dir / name
        if not path.exists():
            path.write_text("{}\n", encoding="utf-8")
