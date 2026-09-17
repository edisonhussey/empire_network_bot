"""Account registry driven by `credentials/*.env`.

Adding an account is a data change, not a code change: drop a new
``credentials/<name>.env`` file next to the existing ones and it automatically
becomes available both as a CLI flag (``--<name>``) and in ``accounts`` output.

Every account keeps its own runtime state (control file, logs, latest server
messages) under ``bot/account_data/<aid>/`` while sharing one database, with
rows isolated by the ``aid`` column.

    python -m bot.cli accounts
    python -m bot.cli --ventrilo proxy status
    python -m bot.cli --pingpoko proxy start
"""

from __future__ import annotations

import argparse
import json
import re
import time
from dataclasses import dataclass
from pathlib import Path
from typing import NoReturn

from . import account_context
from .account_context import AccountContext
from .aid_translation import (
    CREDENTIALS_DIR,
    available_usernames,
    credentials_path,
    read_env,
    resolve_aid,
)


@dataclass(frozen=True)
class Account:
    """One game account and its on-disk runtime locations.

    :attr:`key` is the canonical identity that scopes everything: the account's
    data directory and the ``aid`` column in every table. It is the **login name**
    (``NOM``), because ``AID`` in the login handshake is a *shared portal id* --
    identical for every game account under one login -- and so cannot identify an
    account.

    A raw legacy key can still be passed; then ``username`` is empty and ``key``
    is whatever was given, so previously-created data is not orphaned.
    """

    key: str
    username: str
    server: str
    credentials: str
    context: AccountContext

    @property
    def aid(self) -> str:
        """Alias for :attr:`key`; the database column is still named ``aid``."""

        return self.key

    @property
    def name(self) -> str:
        """Display label: the login name when known, otherwise the raw key."""

        return self.username or self.key

    @property
    def root(self):
        return self.context.root

    @property
    def logs_dir(self):
        return self.context.logs_dir

    @property
    def control_file(self):
        return self.context.control_file

    @property
    def listener_log(self):
        return self.context.listener_log

    @property
    def latest_logs_dir(self):
        return self.context.latest_logs_dir

    def values(self) -> dict[str, str]:
        return read_env(credentials_path(self.username))


def available_accounts() -> tuple[str, ...]:
    return available_usernames()


# ---------------------------------------------------------------------------
# Live login session
# ---------------------------------------------------------------------------

#: Written by the mitmproxy listener when it sees a login handshake. This is the
#: ground truth for "which account is actually logged in right now".
SESSION_FILENAME = "session.json"

#: How long a detected session is trusted. Login tokens expire, and a stale file
#: must never silently select the wrong account.
SESSION_MAX_AGE_SECONDS = 12 * 3600


def session_path() -> Path:
    return account_context.BOT_DIR / "account_data" / SESSION_FILENAME


def write_session(account: Account, *, display_name: str = "", source: str = "lli") -> None:
    """Record which account the live login handshake belongs to.

    Only the account name/id and the in-game display name are stored. Session
    tokens (``LT`` / ``RCT``) are deliberately never written.
    """

    path = session_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "account": account.key,
        "username": account.username,
        "display_name": display_name or account.name,
        "source": source,
        "detected_at": int(time.time()),
    }
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(path)


def read_session(*, max_age_seconds: int = SESSION_MAX_AGE_SECONDS) -> dict[str, object] | None:
    """Read the detected login session, or ``None`` when absent or too old.

    Pass ``max_age_seconds=0`` to ignore the age check.
    """

    path = session_path()
    if not path.exists():
        return None
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    if not isinstance(data, dict):
        return None
    if not (data.get("account") or data.get("aid")):
        return None
    try:
        detected_at = int(data.get("detected_at") or 0)
    except (TypeError, ValueError):
        detected_at = 0
    if max_age_seconds > 0 and detected_at > 0 and time.time() - detected_at > max_age_seconds:
        return None
    return data


def clear_session() -> None:
    try:
        session_path().unlink()
    except FileNotFoundError:
        pass


def session_account(*, max_age_seconds: int = SESSION_MAX_AGE_SECONDS) -> Account | None:
    """The account currently logged in.

    Resolved from the recorded login name first (which is what distinguishes
    accounts), falling back to the recorded scope key.
    """

    session = read_session(max_age_seconds=max_age_seconds)
    if session is None:
        return None
    for candidate in (session.get("account"), session.get("username"), session.get("aid")):
        candidate = str(candidate or "").strip()
        if not candidate:
            continue
        try:
            return load_account(candidate)
        except (FileNotFoundError, ValueError):
            continue
    return None


def session_mismatch(
    account: Account,
    *,
    max_age_seconds: int = SESSION_MAX_AGE_SECONDS,
) -> str | None:
    """Error text when the live session belongs to a *different* account.

    Returns ``None`` when there is no reliable session, or when it matches.
    Callers should refuse to run when this returns a value.
    """

    session = read_session(max_age_seconds=max_age_seconds)
    if session is None:
        return None

    detected_key = str(session.get("account") or session.get("aid") or "").strip()
    detected_user = str(session.get("username") or "").strip()
    if not detected_key and not detected_user:
        return None

    if detected_key and detected_key == account.key:
        return None
    if detected_user and account.username and detected_user == account.username:
        return None

    label = session.get("display_name") or detected_user or detected_key
    return (
        f"live session is '{label}' (account={detected_user or detected_key}) but you selected "
        f"'{account.name}' (account={account.key})"
    )


def load_account(username_or_key: str) -> Account:
    """Resolve a login name (or a raw legacy key) into a concrete :class:`Account`."""

    raw = str(username_or_key).strip()
    if not raw:
        raise ValueError("account name is empty")

    username = find_username_for_login_name(raw)
    if username is not None:
        return _account_for_username(username)

    # No credentials file. Accept a raw key so data created under the old numeric
    # keys is still reachable, but refuse arbitrary typos.
    known_dir = account_context.BOT_DIR / "account_data" / raw
    if raw.isdigit() or known_dir.exists():
        return Account(
            key=raw,
            username="",
            server="",
            credentials="",
            context=account_context.for_key(raw),
        )

    raise FileNotFoundError(
        f"unknown account {raw!r}; expected one of {', '.join(available_accounts()) or 'none'} "
        f"or place a credentials file at {CREDENTIALS_DIR / (raw + '.env')}"
    )


def _account_for_username(username: str) -> Account:
    path = credentials_path(username)
    values = read_env(path) if path.exists() else {}
    return Account(
        key=username,
        username=username,
        server=values.get("SERVER", ""),
        credentials=str(path) if path.exists() else "",
        context=account_context.for_key(username, username),
    )


def legacy_key_map() -> dict[str, str]:
    """Map old numeric scope keys to their login name, derived from credentials.

    Used by the migration that renames existing rows/directories onto the new
    login-name keys.
    """

    mapping: dict[str, str] = {}
    for username in available_accounts():
        try:
            legacy = resolve_aid(username)
        except (FileNotFoundError, ValueError):
            continue
        if legacy and legacy != username:
            mapping[legacy] = username
    return mapping


def normalize_account_name(name: str) -> str:
    """Fold a login name (``NOM``) into a credentials-style slug."""

    return re.sub(r"[^a-z0-9_.-]+", "", str(name).strip().lower())


def find_username_for_login_name(name: str) -> str | None:
    """Match an in-game login name to a ``credentials/*.env`` file.

    Tries the file name first, then the ``USERNAME`` value, case-insensitively.
    """

    wanted = normalize_account_name(name)
    if not wanted:
        return None
    for username in available_accounts():
        if normalize_account_name(username) == wanted:
            return username
        try:
            values = read_env(credentials_path(username))
        except OSError:
            continue
        if normalize_account_name(values.get("USERNAME", "")) == wanted:
            return username
    return None


def create_account_for_login_name(name: str) -> Account:
    """Register a brand new login name so it appears in the tables on first login.

    No account id is invented: the login name *is* the scope key.
    """

    slug = normalize_account_name(name)
    if not slug:
        raise ValueError(f"cannot derive an account name from {name!r}")
    path = credentials_path(slug)
    if path.exists():
        return load_account(slug)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        f"USERNAME = {slug}\n"
        "# Auto-created from the game login handshake.\n"
        "# Add PASSWORD / SERVER here if this account needs a direct login.\n",
        encoding="utf-8",
    )
    return load_account(slug)


def account_for_login_name(name: str, *, create: bool = True) -> Account:
    """Resolve the account behind a login ``NOM``, creating one if necessary."""

    username = find_username_for_login_name(name)
    if username is not None:
        return load_account(username)
    if not create:
        raise FileNotFoundError(f"no credentials entry for login name {name!r}")
    return create_account_for_login_name(name)


def describe_accounts() -> str:
    """Human-readable listing for the CLI."""

    usernames = available_accounts()
    if not usernames:
        return f"no accounts found in {CREDENTIALS_DIR}"
    width = max(len(name) for name in usernames)
    lines = []
    for name in usernames:
        try:
            record = load_account(name)
        except (FileNotFoundError, ValueError) as exc:
            lines.append(f"{name:<{width}}  ERROR: {exc}")
            continue
        lines.append(f"{name:<{width}}  aid={record.aid} server={record.server or '?'} logs={record.logs_dir}")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# CLI integration
# ---------------------------------------------------------------------------


def add_account_arguments(parser: argparse.ArgumentParser, *, required: bool = False) -> None:
    """Add the optional account override, documenting the available shorthands.

    No account flag is needed in normal use: the account is taken from the
    detected login session. The flag exists to override it or to run before
    logging in.
    """

    shorthands = ", ".join(f"--{name}" for name in available_accounts()) or "none found"
    parser.add_argument(
        "--account-name",
        "--username",
        dest="account_name",
        default=None,
        required=required,
        help=(
            "Override the detected login session. Optional - the live session is used "
            f"by default. Known accounts: {shorthands}"
        ),
    )


def split_account_flags(argv: "list[str] | tuple[str, ...]") -> tuple[str | None, list[str]]:
    """Pull ``--<username>`` shorthands out of ``argv``, wherever they appear.

    Handled before argparse runs so the flag works both before and after a
    subcommand, and so subparser defaults cannot clobber it.
    """

    known = set(available_accounts())
    picked: list[str] = []
    remaining: list[str] = []
    for token in argv:
        if token.startswith("--") and token[2:] in known:
            picked.append(token[2:])
            continue
        remaining.append(token)

    if len(picked) > 1:
        raise SystemExit(f"choose one account flag, got: {', '.join('--' + name for name in picked)}")
    return (picked[0] if picked else None), remaining


def apply_account_flags(argv: "list[str]") -> list[str]:
    """Rewrite ``argv`` so a ``--<username>`` shorthand becomes ``--account-name``.

    Call this before ``parser.parse_args`` so the shorthand works in any
    position, e.g. ``--ventrilo proxy start`` and ``proxy start --ventrilo``.
    """

    shorthand, remaining = split_account_flags(argv)
    if shorthand:
        return ["--account-name", shorthand, *remaining]
    return remaining


def account_from_args(args: argparse.Namespace, parser: argparse.ArgumentParser | None = None) -> Account:
    """Resolve the account for this process.

    Precedence:
      1. an explicit ``--account-name`` / ``--<username>`` override
      2. the account detected from the live login session (the normal path)
    """

    def fail(message: str) -> NoReturn:
        if parser is not None:
            parser.error(message)
        raise SystemExit(message)

    username = getattr(args, "account_name", None)
    if not username:
        session = session_account()
        if session is not None:
            return session
        choices = ", ".join(f"--{name}" for name in available_accounts())
        fail(
            "no account is logged in yet. Start the listener and log in so the "
            f"session is detected, or pass an explicit account: {choices or 'none found'}"
        )

    try:
        return load_account(str(username))
    except (FileNotFoundError, ValueError) as exc:
        fail(str(exc))


__all__ = [
    "Account",
    "SESSION_MAX_AGE_SECONDS",
    "account_for_login_name",
    "account_from_args",
    "add_account_arguments",
    "apply_account_flags",
    "available_accounts",
    "clear_session",
    "create_account_for_login_name",
    "describe_accounts",
    "find_username_for_login_name",
    "legacy_key_map",
    "load_account",
    "normalize_account_name",
    "read_session",
    "session_account",
    "session_mismatch",
    "session_path",
    "split_account_flags",
    "write_session",
]
