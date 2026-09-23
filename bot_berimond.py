"""Repeat-attack driver for the Berimond kingdom (`KID = 10`).

This is a *standalone junior driver*: it never edits ``bot.py`` and never sends
packets itself. It reuses ``bot.bot`` read-only for the database, constants and
the ``proxy_control.json`` contract, and it supervises the mitmproxy addon
(``bot/rbc_proxy_listener.py``) which owns the actual ``aci`` -> ``cra`` ->
``cat`` handshake.

Why the addon has to send
-------------------------
Only the addon sits inside the game connection, so only the addon can inject.
``bot.py --mode berimond-proxy`` already proved this contract works (2026-09-04:
``berimond_aci_ok`` -> ``proxy_cra_ack ... travel_duration=179`` ->
``proxy_cat_return ... berimond_attack_updated=True``). What it does *not* do is
know which target is alive: it re-sends one hardcoded coordinate forever.

How to find the active target
----------------------------
Two ways, and the first one needs no coordinates at all.

1. **The find-target button.** The client presses it by sending an empty request
   ``%xt%EmpireEx_21%fnt%1%{}%`` and the server answers with the objective it
   picked, coordinate and row included::

       {"X": 1254, "Y": 110, "gaa": {"KID": 10, "AI": [[17, 1254, 110, ...]]}}

   In the 2026-09-22 captures repeated presses cycled
   ``1256:82 -> 1268:121 -> 1254:110 -> 1239:67``, i.e. the game walks its own
   live objective list for you. This script queues ``kind: "fnt_probe"`` for
   that; the addon sends it (see ``PROBE_KINDS`` in ``rbc_proxy_listener.py``).

2. **A chunk scan** (``--search x1,y1,x2,y2``) sends ``gaa`` with ``KID: 10``
   over 13x13 chunks and decodes every ``AI`` row itself.

A plain ``gaa`` response is also decoded, so simply flying around Berimond in the
real client and then running ``discover --from-logs`` finds targets with no probe
at all. Decoded from those captures:

    [17, x, y, owner, state, links, -1, 50, hp, return_seconds]

* ``row[0] == 17``  attackable objective (camp). ``row[0] == 16`` is a tower and
  ``row[0] == 10`` a resource village; ``row[0] == 31`` is empty terrain.
* ``row[4]``        0 = alive/attackable, 1 = **defeated**. This is the flag to
  rotate on. It flips to 1 once ``hp`` is exhausted, e.g. ``(1268,121)``:
  ``hp 19 -> 7 -> 6 -> 2 -> 1 state=1``.
* ``row[5]``        links to child objectives, i.e. the objective tree:
  ``(1242,113) -> (1254,110) -> (1268,121)``.
* ``row[8]``        remaining hp. It only ever falls, and it falls in irregular
  steps because the pool is shared with everyone else attacking the event, so
  "defeated after X attacks" cannot be predicted - it has to be re-probed.
* ``row[9]``        return/repeat time for that objective in seconds. Observed to
  equal the ``cat`` ``TT`` field exactly (36 for ``1254:110``), which is why an
  attack can come back "instantly" while the outbound leg took ~170s.

Cooldowns
---------
Two limits, both enforced here and by the addon:

* **Per commander** - the addon marks the lord busy until
  ``sent_at + HEURISTIC_RETURN_SECONDS`` capped by
  ``berimond_max_commander_out_seconds``, then shortens it to
  ``cat_at + TT + hold`` when the return packet arrives. So a 36s return leg
  really does free the commander in ~46-56s. This script only *reads* that
  state (``commander_state.available_after``) and waits for the earliest lord.
* **Between attacks** - ``max(cra_due, last_send + 4.0 + jitter)``, so two CRAs
  are never closer than ~4.15s, and the ACI always precedes its CRA by
  ``2.5 + jitter`` seconds (``Randomizer.berimond_adi_to_cra_waiting_time``).
  Berimond sends **no ADI** - that is the Sands/Storm handshake; the pair here
  is ACI -> CRA.

Rejections
----------
The addon logs ``proxy_cra_error status=.. reason=.. target=x:y lid=..`` for a
refused CRA; this script reads those lines and classifies them
(``classify_cra_error``):

* ``not_enough_troops`` -> the army is the problem.
* ``lord_in_use`` -> the commander is the problem.
* anything else (every berimond rejection on record so far is ``status=101
  reason=unknown``) -> one find-target press decides it: if the game still names
  the camp we attacked the failure is ours and the alert sound is raised; if it
  names a different camp the target moved, so that one is adopted and its
  cooldown starts fresh. With no map answer the class stays ``unverified``
  rather than guessing.

The addon stops the run on some of these and tolerates others (its own budget is
``berimond_max_cra_errors`` per ``berimond_cra_error_window_seconds``). This
script owns the tighter policy on top: ``--error-tolerance`` (default 1) rejects
tolerated errors in one sequence - sequence meaning no attack has been sent since
the last rejection - and the next one closes the run with
``error_budget_exhausted``. Tolerated errors back off per class
(``error_backoff_seconds``): 180s for ``army`` because a camp that is out of
troops only recovers when marches return or when troops are refilled by hand,
30s for ``commander``, 60s for ``unverified``, and none for a retarget.

A refusal can also arrive before any CRA, at the ``aci`` handshake itself: the
server answers ``%xt%aci%1%203%`` with an empty body when the camp cannot be
attacked any more and the addon stops the transport (``stop_reason =
berimond_aci_status_203``). That is a fact about one camp, not a verdict on the
run, so it is handled like a rejection: ask the game where it would send us now,
adopt a different camp with a fresh cooldown, and only count it against the error
budget when the game still names the same camp (``berimond_aci_rejected`` /
``berimond_aci_retarget`` / ``aci_needs_attention`` in the log). Without that, a
stale camp kills the run in about a second.

Starting
--------
``run`` presses **find target before anything else** and picks from the rows the
game hands back (``startup_lookup live=..``), because the camp a previous run left
in the cache may already be defeated - the 14:10 and 14:15 runs on 2026-09-22
died in 1-2s that way, having inherited ``1201:96`` from the run before it. The
cache is only the fallback when the probe cannot answer (addon not loaded, game
not connected), and that fallback is logged as ``startup_lookup_empty``.
``--target x:y`` pins a camp and skips the lookup (``startup_lookup_skipped``).

Army
----
The attack itself is ``bot/berimond.py::ATTACK`` (mantlet 620 x30 plus marksman
x4 and veteran deathly horror x22 = 26 units); the addon builds it, so this
script deliberately does not touch it. The manual hit at 11:09 on 2026-09-22 was
620 x30 with 26 of troop 14, so if you want that exact stack, edit ``ATTACK`` in
``bot/berimond.py`` and let the addon reload - not this file.

Refilling the camp
-----------------
Attacking costs troops: measured 2026-09-22, a wave of 14 VDH + 12 marksmen
came back as 8.5 + 7.3, so ~10 die per attack (39%), and the camp's 302 VDH
capacity also has up to 224 in flight at once. The camp therefore runs dry.

The refill is the game's own ``kut`` transfer, captured 2026-09-22 10:49:43::

    %xt%EmpireEx_21%kut%1%{"SCID":16011862,"SKID":0,"TKID":10,"CID":-1,
      "A":[[10,302],[620,705349]]}%

The answer is ``{"kpi":{"UT":[{"KID":10,"RS":7200,"I":[...]}]}}`` - ``RS``
is the 7200s march the player skips by hand.

On a troop rejection this script measures what to send - nothing is hardcoded:

    away      = (in_flight + resolved) * sent_per_attack
    now       = home + landed - away                 # what the camp holds right now
    total     = now + back + out                     # once the flyers land
    missing   = capacity - total
    send      = floor(safety * missing)              # safety defaults to 0.9

``home``/``landed`` come from the stock snapshot (``aci`` on every attack, or
``gui``) and the ``cat`` returns whose ``arrives_at`` passed after it, ``back``
is the survivors on their way home right now, and ``out`` estimates the marches
whose result has not come back yet with the measured survival ratio (an unknown
ratio counts them as lost). ``away`` is the part a naive sum misses: the marches
that are out took their troops from the camp, so they come off the snapshot - a
camp reading 260 with 25 marches of 14 out holds ~20, not 526 (and anything above
the camp's capacity is the tell that the sum is wrong). The counters are taken
relative to the snapshot, because a fresh ``aci`` capture already includes the
marches that left before it. ``capacity`` comes from ``bot/berimond.py``
(``TOTAL_CAPACITY``; ``CAPACITY_BY_UNIT`` is authoritative when it lists units).
The source castle is learned from the last ``kut`` the run has seen rather than
hardcoded.

A queued refill *replaces* the army backoff: the run is already waiting for the
troops to arrive (``--refill-wait``/``RS``), so ``army_backoff`` would only be
180s of extra sleep - and it used to overwrite that wait. The 180s still applies
when there is nothing to send (``--no-refill``, no source castle, no stock).

It then waits for arrival - ``RS`` from the game unless ``--refill-wait`` says
otherwise - and resumes. Flags: ``--refill-units "10"`` (unit ids; a ``10:302``
form still parses but the count is ignored), ``--refill-safety``,
``--refill-castle-id``, ``--refill-wait``, ``--refill-max-wait``,
``--refill-stock-max-age``. Speed-ups are not automated (which packet that is has
not been identified yet), so unattended the run waits the full march.

Usage
-----
Shared options go *before* the subcommand, exactly like ``python -m bot.cli``:

    # what is alive right now (no probe, just parse recent captures)
    python bot_berimond.py --ventrilo --no-db --minutes 30 discover --from-logs

    # ask the game for the map (needs the addon loaded and idle)
    python bot_berimond.py --ventrilo discover --search 1240,60,1290,130

    # run it
    python bot_berimond.py --ventrilo --commander-count 16 run --max-attacks 200

``run`` writes intent into the account control file and polls it; stop it with
Ctrl-C and it stops the addon transport before exiting. It refuses to start
(without writing anything) if the account session does not match the configured
account.
"""

from __future__ import annotations

import argparse
import json
import re
import signal
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any, Iterable


def find_repo_root(path: Path) -> Path:
    for parent in (path, *path.parents):
        if (parent / "bot" / "scheduler.py").is_file():
            return parent
    return path.parents[1]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from bot import accounts  # noqa: E402
from bot import berimond  # noqa: E402
from bot import bot as runner  # noqa: E402
from bot.packets import MAP_CHUNK_SIZE, army_requirements, parse_xt_packet  # noqa: E402


KID = berimond.KID
OBJECTIVE_ROW = 17
TOWER_ROW = 16
RESOURCE_ROW = 10

#: Travel option proven accepted for Berimond by the 2026-09-22 client capture
#: (``"HBW": 1021`` in the outbound ``cra``). ``bot/berimond.py`` still carries
#: the ``-1`` placeholder, so it is overridden here instead of edited there.
DEFAULT_HBW = 1021
#: Return leg of the ``cra`` ack is per commander and needs a fresh command file
#: read, so allow a couple of addon polls.
PROBE_TIMEOUT_SECONDS = 25.0
#: A refill transfer can be queued right when the addon's own pacing pause
#: (``REQUEST_INTERVAL_RANGE``, 20-30s) is still running, so its timeout has to
#: outlast that pause - 25s was a coin toss.
TRANSFER_TIMEOUT_SECONDS = 45.0
CONTROL_POLL_SECONDS = 1.0
#: A pending write can be clobbered by the addon's own ``save_control``; only
#: touch the file when the addon is idle, then read back and retry.
WRITE_ATTEMPTS = 6
IDLE_WINDOW_SECONDS = 3.0


class StopRequested(Exception):
    """Ctrl-C arrived - abandon whatever wait is running and exit cleanly.

    The signal handler can only set a flag, and a flag is useless to a probe that
    is 20s into a 25s deadline (2026-09-23: the user pressed Ctrl-C five times and
    the run still took another 19s to exit). The wait loops call ``check_stop()``
    so the interrupt lands within a poll instead.
    """


_STOP = False


def request_stop() -> None:
    """Ask the run to stop at the next check (called from the signal handler)."""

    global _STOP
    _STOP = True


def stop_requested() -> bool:
    return _STOP


def check_stop() -> None:
    """Raise :class:`StopRequested` when a stop has been asked for."""

    if _STOP:
        raise StopRequested()

#: Polling the commander roster once a second for hours is wasteful; a few
#: seconds of staleness costs nothing because the addon does its own gating.
ROSTER_POLL_SECONDS = 3.0
BUSY_LOG_SECONDS = 30.0

#: Rejection reasons that are about *us* and not about the target.
#: ``313 not_enough_troops`` and ``256 lord_in_use`` come from the game's own
#: error text (see ``CRA_STATUS_REASONS`` in the addon).
ARMY_ERROR_REASONS = frozenset({"not_enough_troops"})
COMMANDER_ERROR_REASONS = frozenset({"lord_in_use"})

TARGET_FILE_NAME = "berimond_targets.json"
RUN_LOG_NAME = "bot_berimond.log"


class ProbeError(RuntimeError):
    """The addon could not be made to send a read-only map probe."""


# ---------------------------------------------------------------------------
# Objectives
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Objective:
    """One ``AI`` row of type 17 from a Berimond ``gaa`` response."""

    x: int
    y: int
    state: int
    hp: int
    return_seconds: int
    owner: int | None = None
    links: tuple[tuple[int, int], ...] = ()
    seen_at: float = 0.0
    source_file: str = ""

    @property
    def alive(self) -> bool:
        return self.state == 0

    @property
    def key(self) -> str:
        return f"{self.x}:{self.y}"

    def as_dict(self) -> dict[str, Any]:
        return {
            "x": self.x,
            "y": self.y,
            "state": self.state,
            "hp": self.hp,
            "return_seconds": self.return_seconds,
            "owner": self.owner,
            "links": [list(link) for link in self.links],
            "seen_at": self.seen_at,
            "source_file": self.source_file,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "Objective":
        links = tuple(
            (int(link[0]), int(link[1]))
            for link in data.get("links") or []
            if isinstance(link, (list, tuple)) and len(link) >= 2
        )
        return cls(
            x=int(data["x"]),
            y=int(data["y"]),
            state=int(data.get("state", 0) or 0),
            hp=int(data.get("hp", 0) or 0),
            return_seconds=int(data.get("return_seconds", 0) or 0),
            owner=data.get("owner"),
            links=links,
            seen_at=float(data.get("seen_at", 0.0) or 0.0),
            source_file=str(data.get("source_file") or ""),
        )


def _objective_from_row(row: list[Any]) -> Objective | None:
    try:
        x = int(row[1])
        y = int(row[2])
    except (IndexError, TypeError, ValueError):
        return None
    state = _int_or(row[4], 0)
    hp = _int_or(row[8], 0)
    return_seconds = _int_or(row[9], 0)
    owner = _int_or(row[3], None)
    links: list[tuple[int, int]] = []
    raw_links = row[5] if len(row) > 5 else None
    if isinstance(raw_links, list):
        for link in raw_links:
            if isinstance(link, (list, tuple)) and len(link) >= 2:
                try:
                    links.append((int(link[0]), int(link[1])))
                except (TypeError, ValueError):
                    continue
    return Objective(
        x=x,
        y=y,
        state=state,
        hp=hp,
        return_seconds=return_seconds,
        owner=owner,
        links=tuple(links),
    )


def _int_or(value: Any, default: Any) -> Any:
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def objectives_from_gaa(payload: dict[str, Any]) -> list[Objective]:
    """Live objectives from a Berimond ``gaa`` payload (``AI`` rows, type 17)."""

    objectives: list[Objective] = []
    for row in payload.get("AI") or []:
        if not isinstance(row, list) or not row:
            continue
        if _int_or(row[0], None) != OBJECTIVE_ROW:
            continue
        objective = _objective_from_row(row)
        if objective is not None:
            objectives.append(objective)
    return objectives


def objectives_from_fnt(payload: dict[str, Any]) -> list[Objective]:
    """Objectives from an ``fnt`` ("find target") response.

    Pressing the button sends ``%xt%EmpireEx_21%fnt%1%{}%`` and the server
    answers with the objective it picked::

        {"X": 1254, "Y": 110, "gaa": {"KID": 10, "AI": [[17, 1254, 110, ...]]}}

    So one empty request is enough - no coordinates and no chunk maths. When the
    nested window has no type-17 row (the 10:41 capture answered ``KID: 3`` with
    an empty ``AI``, i.e. the player was not in Berimond) nothing is reported.
    """

    inner = payload.get("gaa")
    if isinstance(inner, dict):
        if _int_or(inner.get("KID"), None) != KID:
            return []
        rows = objectives_from_gaa(inner)
        if rows:
            return rows
    x = _int_or(payload.get("X"), None)
    y = _int_or(payload.get("Y"), None)
    if x is None or y is None:
        return []
    # The button named an objective but the window had no row for it; keep it
    # with unknown progress rather than dropping the game's own answer.
    return [Objective(x=int(x), y=int(y), state=0, hp=0, return_seconds=0)]


def pick_target(
    objectives: Iterable[Objective],
    *,
    mode: str,
    source: tuple[int, int] | None,
    exclude: Iterable[str] = (),
) -> Objective | None:
    """Choose the next objective to hit.

    ``weakest`` finishes whatever is nearly defeated (fewest attacks wasted when
    the pool is shared), ``nearest`` minimises the trip from our camp, and
    ``first`` keeps the order the game returned.
    """

    skipped = {str(key) for key in exclude}
    live = [o for o in objectives if o.alive and o.key not in skipped]
    if not live:
        return None
    if mode == "first":
        return live[0]
    if mode == "nearest" and source is not None:
        return min(live, key=lambda o: (abs(o.x - source[0]) + abs(o.y - source[1]), o.hp))
    # weakest (default): lowest hp, then shortest return leg.
    return min(live, key=lambda o: (o.hp, o.return_seconds, o.key))


# ---------------------------------------------------------------------------
# Capture logs
# ---------------------------------------------------------------------------


def _block_timestamp(path: Path, stamp: str | None) -> float:
    """Epoch for a capture block, using the date embedded in the file name."""

    if stamp:
        try:
            date_text = path.name.split("_", 2)[1]
            base = datetime.strptime(date_text, "%Y%m%d")
            clock = datetime.strptime(stamp.split(",")[0], "%H:%M:%S.%f")
            return (
                base.replace(hour=clock.hour, minute=clock.minute, second=clock.second)
                + timedelta(microseconds=clock.microsecond)
            ).timestamp()
        except (IndexError, ValueError):
            pass
    try:
        return path.stat().st_mtime
    except OSError:
        return 0.0


def iter_capture_packets(logs_dir: Path, *, minutes: float | None, limit_files: int) -> Iterable[tuple[float, str, str]]:
    """Yield ``(seen_at, file_name, raw_packet)`` oldest first from captures."""

    if not logs_dir.is_dir():
        return
    cutoff = 0.0 if minutes is None else time.time() - float(minutes) * 60.0
    paths = [
        path
        for path in logs_dir.glob("gge_*.log")
        if path.stat().st_mtime >= cutoff
    ]
    paths.sort(key=lambda item: item.stat().st_mtime)
    for path in paths[-limit_files:]:
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for block in text.split("---"):
            stamp = None
            raw = None
            for line in block.splitlines():
                stripped = line.strip()
                if stamp is None and stripped.startswith("[") and "]" in stripped:
                    stamp = stripped.strip("[]").split("]")[0]
                if raw is None and stripped.startswith("%xt%"):
                    raw = stripped
            if raw is None:
                continue
            yield _block_timestamp(path, stamp), path.name, raw


def objectives_since(
    logs_dir: Path,
    *,
    since: float | None,
    minutes: float | None = 60.0,
    limit_files: int = 600,
) -> dict[str, Objective]:
    """Newest observation per objective coordinate, optionally only after ``since``."""

    newest: dict[str, Objective] = {}
    for seen_at, name, raw in iter_capture_packets(logs_dir, minutes=minutes, limit_files=limit_files):
        if since is not None and seen_at < since:
            continue
        if "%gaa%" not in raw:
            continue
        parsed = parse_xt_packet(raw)
        if not parsed or parsed.get("command") != "gaa":
            continue
        payload = parsed.get("payload")
        if not isinstance(payload, dict) or _int_or(payload.get("KID"), None) != KID:
            continue
        for objective in objectives_from_gaa(payload):
            stamped = Objective(**{**objective.__dict__, "seen_at": seen_at, "source_file": name})
            previous = newest.get(stamped.key)
            if previous is None or stamped.seen_at >= previous.seen_at:
                newest[stamped.key] = stamped
    return newest


def objectives_from_logs(
    logs_dir: Path,
    *,
    minutes: float | None = 60.0,
    limit_files: int = 600,
) -> dict[str, Objective]:
    return objectives_since(logs_dir, since=None, minutes=minutes, limit_files=limit_files)


def fnt_targets_since(logs_dir: Path, *, since: float, limit_files: int = 12) -> dict[str, Objective]:
    """Newest find-target answer per coordinate, from captures written after ``since``."""

    newest: dict[str, Objective] = {}
    for seen_at, name, raw in iter_capture_packets(logs_dir, minutes=None, limit_files=limit_files):
        if seen_at < since or "%fnt%" not in raw:
            continue
        parsed = parse_xt_packet(raw)
        if not parsed or parsed.get("command") != "fnt":
            continue
        payload = parsed.get("payload")
        if not isinstance(payload, dict):
            continue
        for objective in objectives_from_fnt(payload):
            stamped = Objective(**{**objective.__dict__, "seen_at": seen_at, "source_file": name})
            previous = newest.get(stamped.key)
            if previous is None or stamped.seen_at >= previous.seen_at:
                newest[stamped.key] = stamped
    return newest


# ---------------------------------------------------------------------------
# Control file
# ---------------------------------------------------------------------------


class Control:
    """Thin, verified wrapper around the addon's ``proxy_control.json``.

    ``listener_log`` is the addon's own log, used to fail fast when it reports
    that it cannot send (no websocket) instead of waiting out the timeout.
    """

    def __init__(self, path: Path, listener_log: Path | None = None) -> None:
        self.path = path
        self.listener_log = listener_log

    def load(self) -> dict[str, Any]:
        try:
            data = json.loads(self.path.read_text(encoding="utf-8"))
        except (FileNotFoundError, json.JSONDecodeError):
            return {}
        return data if isinstance(data, dict) else {}

    def save(self, state: dict[str, Any]) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(self.path.suffix + ".tmp")
        tmp.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        tmp.replace(self.path)

    def update(self, mutate) -> dict[str, Any]:
        """Read-modify-write, so keys the addon owns are never dropped."""

        state = self.load()
        mutate(state)
        self.save(state)
        return state

    def idle(self) -> bool:
        """True when a write cannot drop an attack the addon is driving."""

        state = self.load()
        if isinstance(state.get("pending"), dict):
            return False
        if state.get("running") and state.get("mode") not in (None, "", "berimond"):
            # A sands/storm run owns the addon; probing would interleave with it.
            return False
        return True

    def quiet_for(self, seconds: float) -> bool:
        """True when no attack was sent recently (``last_cra`` is the marker)."""

        last = self.load().get("last_cra")
        if not isinstance(last, dict):
            return True
        sent_at = last.get("sent_at")
        if not isinstance(sent_at, (int, float)):
            return True
        return time.time() - float(sent_at) >= float(seconds)


def wait_for_idle(control: Control, timeout: float) -> bool:
    """Wait until the addon is between attacks, so a probe cannot drop one."""

    deadline = time.time() + timeout
    while time.time() < deadline:
        check_stop()
        if control.idle() and control.quiet_for(IDLE_WINDOW_SECONDS):
            return True
        time.sleep(CONTROL_POLL_SECONDS)
    return False


def log_size(path: Path | None) -> int:
    try:
        return int(path.stat().st_size) if path is not None else 0
    except OSError:
        return 0


def log_since(path: Path | None, offset: int) -> str:
    if path is None:
        return ""
    try:
        with path.open("r", encoding="utf-8", errors="replace") as handle:
            handle.seek(offset)
            return handle.read()
    except OSError:
        return ""


def classify_cra_error(
    error: dict[str, Any],
    *,
    believed: str,
    live: dict[str, Objective],
    pick: str = "weakest",
    source: tuple[int, int] | None = None,
    exclude: Iterable[str] = (),
) -> tuple[str, Objective | None]:
    """Decide what a berimond rejection means. Returns ``(class, replacement)``.

    The rule, in the order it is applied:

    1. ``not_enough_troops`` (status 313) is the army -> ``army``.
    2. ``lord_in_use`` (status 256) is the commander -> ``commander``.
    3. Otherwise ask the map: ``live`` is the answer of one find-target press.

       * the game still names the camp we attacked -> the failure is ours
         (army/tool composition) -> ``army``.
       * it names a different camp -> the target moved -> ``target_changed``
         together with the objective to attack next.

    4. No usable map answer -> ``unverified``; never guess.

    ``reported`` (the coordinate in the rejection) differing from ``believed``
    is treated as the target having moved as well, but the replacement still has
    to come from a live answer - the rejection only carries one coordinate.
    """

    reason = str(error.get("reason") or "unknown")
    reported = error.get("target")
    if reason in ARMY_ERROR_REASONS:
        return "army", None
    if reason in COMMANDER_ERROR_REASONS:
        return "commander", None
    mismatched = reported is not None and reported != believed
    if not live:
        return "unverified", None
    if believed in live and not mismatched:
        return "army", None
    replacement = pick_target(
        live.values(), mode=pick, source=source, exclude=set(exclude) | {believed}
    )
    if replacement is None:
        return "unverified", None
    return "target_changed", replacement


ACI_STOP_PREFIX = "berimond_aci_status_"


def is_aci_rejection(stop_reason: Any) -> bool:
    """True when the addon stopped because the server refused to start the attack.

    The addon writes ``stop_reason = berimond_aci_status_<status>`` and clears its
    pending attack (``process_pending_berimond_aci_error``). ``%xt%aci%1%203%``
    with an empty body is what a camp that can no longer be attacked answers, so
    this is a rejection of one target - not a failure of the run.
    """

    return str(stop_reason or "").startswith(ACI_STOP_PREFIX)


def aci_status(stop_reason: Any) -> str:
    """The numeric ACI status inside a ``berimond_aci_status_*`` stop reason."""

    text = str(stop_reason or "")
    if not text.startswith(ACI_STOP_PREFIX):
        return "unknown"
    return text[len(ACI_STOP_PREFIX) :] or "unknown"


def error_backoff_seconds(
    classification: str,
    *,
    army_backoff: float = 180.0,
    error_pause: float = 30.0,
) -> float:
    """How long to hold off before retrying after a tolerated rejection.

    An ``army`` rejection is usually the camp being out of troops, and troops
    only come back when marches return (or when they are refilled by hand), so
    it gets a much longer wait than a commander bookkeeping clash. A retarget
    needs no wait at all - the coordinate already changed.
    """

    if classification == "army":
        return max(0.0, float(army_backoff))
    if classification == "commander":
        return max(0.0, float(error_pause))
    if classification == "unverified":
        return max(0.0, float(error_pause) * 2.0)
    return 0.0


def army_need() -> dict[int, int]:
    """Units one berimond attack takes, straight from ``bot/berimond.py``."""

    return dict(army_requirements(berimond.ATTACK.to_payload()))


def camp_stock(state: dict[str, Any]) -> tuple[dict[int, int], int]:
    """``(unit_id: count, captured_at)`` from the addon's ``gui`` capture.

    ``gui`` is the only view of the Berimond camp's stock, and it is captured
    whenever the client asks for it - so it can be stale, and a stale one must
    never gate anything. It is used for reporting only.
    """

    raw = state.get("berimond_stock")
    if not isinstance(raw, dict):
        return {}, 0
    captured = int(raw.get("at", 0) or 0)
    inventory: dict[int, int] = {}
    for unit_id, count in (raw.get("inventory") or {}).items():
        try:
            inventory[int(unit_id)] = int(count)
        except (TypeError, ValueError):
            continue
    return inventory, captured


def describe_camp_stock(state: dict[str, Any], log) -> None:
    """Log the camp's captured stock against what one attack needs.

    This is the line to read when troops run out: it says exactly which unit is
    short and by how much, using the game's own numbers rather than a guess.
    """

    inventory, captured = camp_stock(state)
    need = army_need()
    if not inventory:
        log("berimond_camp_stock unknown (no gui capture yet; open the camp's troop view)")
        return
    age = max(0, int(time.time()) - captured) if captured else -1
    have = " ".join(f"u{unit_id}={inventory.get(unit_id, 0)}" for unit_id in sorted(need))
    want = " ".join(f"u{unit_id}={count}" for unit_id, count in sorted(need.items()))
    log(f"berimond_camp_stock age={age}s need={want} have={have}")
    short = [unit_id for unit_id, count in need.items() if inventory.get(unit_id, 0) < count]
    if short:
        log(
            "berimond_stock_low "
            + " ".join(f"u{unit_id}={inventory.get(unit_id, 0)}/{need[unit_id]}" for unit_id in sorted(short))
            + " -> refill the camp before the next attack"
        )


def learned_refill_source(logs_dir: Path, *, max_files: int = 1500, cache_path: Path | None = None) -> tuple[int, int] | None:
    """``(source castle id, source kingdom)`` for a refill transfer.

    Taken from the newest ``kut`` in the captures - the transfer the player makes
    by hand, e.g. ``{"SCID": 16011862, "SKID": 0, "TKID": 10, "A": [[10, 302]]}``
    - so no castle id is hardcoded. Captures are walked newest-first and the scan
    stops at the first file containing one, then the answer is cached because it
    is a stable fact about the account.
    """

    if cache_path is not None:
        try:
            cached = json.loads(cache_path.read_text(encoding="utf-8"))
        except (FileNotFoundError, json.JSONDecodeError):
            cached = None
        if isinstance(cached, dict) and cached.get("scid") is not None:
            return int(cached["scid"]), int(cached.get("skid", 0))

    if not logs_dir.is_dir():
        return None
    try:
        paths = sorted(logs_dir.glob("gge_*.log"), key=lambda item: item.stat().st_mtime, reverse=True)[:max_files]
    except OSError:
        return None
    for path in paths:
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "%kut%" not in text or "SCID" not in text:
            continue
        found: tuple[int, int] | None = None
        for block in reversed(text.split("---")):
            raw = None
            for line in block.splitlines():
                stripped = line.strip()
                if stripped.startswith("%xt%") and raw is None:
                    raw = stripped
            if raw is None or "%kut%" not in raw or "SCID" not in raw:
                continue
            parsed = parse_xt_packet(raw)
            payload = parsed.get("payload") if parsed else None
            if not isinstance(payload, dict) or payload.get("SCID") is None:
                continue
            try:
                found = (int(payload["SCID"]), int(payload.get("SKID", 0)))
            except (TypeError, ValueError):
                continue
            break
        if found is None:
            continue
        if cache_path is not None:
            try:
                cache_path.write_text(
                    json.dumps({"scid": found[0], "skid": found[1], "source_file": path.name}, indent=2)
                    + "\n",
                    encoding="utf-8",
                )
            except OSError:
                pass
        return found
    return None


def capacity_for(unit_id: int) -> int:
    """How many of ``unit_id`` the camp can hold (``bot/berimond.py``).

    ``TOTAL_CAPACITY`` is the camp's stock for the unit being attacked. When
    ``CAPACITY_BY_UNIT`` lists units it is authoritative, so a unit that is not in
    it is *not* refilled (rather than being given the camp's troop capacity by
    mistake). Zero means "do not refill this one".
    """

    overrides = getattr(berimond, "CAPACITY_BY_UNIT", None)
    if isinstance(overrides, dict) and overrides:
        return int(overrides.get(int(unit_id), 0) or 0)
    return int(getattr(berimond, "TOTAL_CAPACITY", 0) or 0)


def survival_ratios(state: dict[str, Any], sent: dict[int, int]) -> dict[int, float]:
    """Measured fraction of each unit that comes back, from the ``cat`` returns.

    ``berimond_returns`` holds one entry per returned march with the survivors in
    ``units``; every one of those marches carried the same army (``sent``), so
    ``returned / sent`` over the recorded returns is a real ratio rather than a
    guess. Empty when nothing has come back yet - callers must cope.
    """

    returns = [item for item in (state.get("berimond_returns") or []) if isinstance(item, dict)]
    if not returns:
        return {}
    ratios: dict[int, float] = {}
    for unit_id in sent:
        returned = 0
        for item in returns:
            for unit_text, count in (item.get("units") or {}).items():
                try:
                    if int(unit_text) == int(unit_id):
                        returned += int(count)
                except (TypeError, ValueError):
                    continue
        carried = int(sent[unit_id]) * len(returns)
        if carried > 0:
            ratios[int(unit_id)] = min(1.0, returned / carried)
    return ratios


def returned_units(item: dict[str, Any]) -> dict[int, int]:
    """``unit_id: count`` from one ``berimond_returns`` entry."""

    out: dict[int, int] = {}
    for unit_text, count in (item.get("units") or {}).items():
        try:
            out[int(unit_text)] = int(count)
        except (TypeError, ValueError):
            continue
    return out


def expected_returning(unit_id: int, sent: dict[int, int], in_flight: int, ratios: dict[int, float]) -> float:
    """How many of ``unit_id`` the marches that are still out are expected to bring back.

    Zero when no ratio is known yet - that is the conservative direction, the
    estimate then assumes those troops are gone and tops the camp up further.
    """

    return float(int(in_flight) * int(sent.get(int(unit_id), 0)) * float(ratios.get(int(unit_id), 0.0)))


def camp_estimate(
    unit_id: int,
    *,
    sent: dict[int, int],
    home: dict[int, int],
    captured: int,
    in_flight: int,
    resolved: int = 0,
    ratios: dict[int, float],
    returns: list[dict[str, Any]],
    now: float,
) -> dict[str, float]:
    """What the camp holds now, and what it will hold once everything lands.

    This is a *balance* from the ``gui``/``aci`` snapshot, which is why it does
    not simply add the survivors up::

        away   = (in_flight + resolved) * sent_per_attack   # marches take troops OUT
        now    = home + landed - away                       # what is in the camp right now
        total  = now + back + out                           # once the flyers land

    ``away`` is the part that used to be missing: the snapshot cannot contain
    troops that left the camp after it was taken, so they have to come off. That
    is the in-flight lockup - a camp reading 260 with 25 marches of 14 out is at
    ~20, not at 526, and 526 is what a naive sum reports (it is also above the
    camp's 302 capacity, which is the tell that the sum is wrong).

    * ``landed`` - survivors from ``cat`` returns that came home after the snapshot
      (already inside the stock, so not counted twice);
    * ``back``   - survivors on their way home right now (``arrives_at`` ahead);
    * ``out``    - marches whose result has not come back yet, estimated with the
      measured survival ratio (unknown ratio -> counted as lost).
    """

    landed = 0
    back = 0
    for item in returns:
        count = returned_units(item).get(int(unit_id), 0)
        if count <= 0:
            continue
        arrives = float(item.get("arrives_at", 0) or 0)
        if arrives > float(now):
            back += count
        elif captured and arrives > float(captured):
            landed += count
    out = expected_returning(unit_id, sent, in_flight, ratios)
    per_attack = int(sent.get(int(unit_id), 0))
    away = (int(in_flight) + int(resolved)) * per_attack
    home_count = int(home.get(int(unit_id), 0))
    current = max(0.0, home_count + landed - away)
    return {
        "home": float(home_count),
        "landed": float(landed),
        "away": float(away),
        "now": current,
        "back": float(back),
        "out": float(out),
        "total": current + back + out,
    }


def plan_refill(
    *,
    sent: dict[int, int],
    expected: dict[int, float],
    safety: float = 0.9,
) -> list[list[int]]:
    """How many troops to push over, ``safety`` of what is missing.

    ``missing = capacity - expected``, i.e. the capacity from ``bot/berimond.py``
    minus everything the camp already holds or will hold once the marches that
    are out come home (``camp_estimate``). Only ``safety`` of that gap is sent,
    so an overfill - and a wasted 2h march - is avoided.
    """

    plan: list[list[int]] = []
    for unit_id in sorted(sent):
        capacity = capacity_for(unit_id)
        if capacity <= 0:
            continue
        missing = capacity - float(expected.get(int(unit_id), 0.0))
        if missing <= 0:
            continue
        amount = int(missing * max(0.0, min(1.0, float(safety))))
        if amount > 0:
            plan.append([int(unit_id), amount])
    return plan


def refill_transfer(*, units: dict[int, int], stock: dict[int, int]) -> list[list[int]]:
    """What to send: top each unit up to its target, never send a shortfall twice.

    ``units`` is ``{unit_id: target}`` and ``stock`` the camp's last captured
    stock, so a camp at 299/302 sends 3 rather than another 302.
    """

    send: list[list[int]] = []
    for unit_id, target in sorted(units.items()):
        missing = int(target) - int(stock.get(unit_id, 0))
        if missing > 0:
            send.append([int(unit_id), missing])
    return send


def parse_cra_skip(line: str) -> str | None:
    """The reason from the addon's ``proxy_cra_skip reason=..`` line.

    A skip means no attack left the camp at all, so a run of them with
    ``attacks_sent`` still at zero is a stall, not progress.
    """

    if "proxy_cra_skip reason=" not in line:
        return None
    match = re.search(r"proxy_cra_skip reason=(\S+)", line)
    return match.group(1) if match else "unknown"


def parse_cra_error(line: str) -> dict[str, Any] | None:
    """Parse the addon's berimond rejection line.

    Shape (``proxy_cra_error status=5 reason=action_could_not_be_performed
    target=1254:110 lid=0 task=berimond_fixed ...``), written by
    ``process_live_cra_response``. The ``_tolerated`` variant is skipped on
    purpose - only the real error line carries the target.
    """

    if "proxy_cra_error status=" not in line:
        return None
    error: dict[str, Any] = {"raw": line.strip()}
    for key, pattern in (
        ("status", r"status=(\S+)"),
        ("reason", r"reason=(\S+)"),
        ("target", r"target=(\d+):(\d+)"),
        ("lid", r"lid=(\S+)"),
        ("task", r"task=(\S+)"),
    ):
        for match in re.finditer(pattern, line):
            error[key] = f"{match.group(1)}:{match.group(2)}" if key == "target" else match.group(1)
    return error


def play_alert(reason: str, log) -> None:
    """Raise the berimond alert sound ourselves when the addon did not.

    The addon plays it for berimond rejections and ACI failures, so this is only
    reached when its alert is switched off (``berimond_alert_on_error`` false) or
    the sound line is missing - the two never double-beep.
    """

    path = str(berimond.ALERT_SOUND_PATH)
    try:
        subprocess.Popen(
            ["afplay", path],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        log(f"berimond_alert_sound reason={reason} path={path} played_by=bot_berimond")
    except OSError as exc:
        log(f"berimond_alert_sound_failed reason={reason} error={exc!r}")


def reset_global_cooldown(control: Control) -> None:
    """Give the next attack a fresh global window.

    A rejected CRA never became an attack, so the new target should not inherit
    the failed send's ``last_global_attack_sent_at``. The addon's own 20-30s
    post-error pause still applies on top, so this cannot turn into a burst.
    """

    control.update(lambda state: state.__setitem__("last_global_attack_sent_at", None))


def addon_binding_hint(control: Control) -> str:
    """Explain an unanswered probe from the tail of the addon's own log.

    Two failure modes look identical from the control file: the script is not
    loaded at all, or it never saw a login and is therefore reading some other
    account's control file (``control_candidates()`` is alphabetical, so
    ``pingpoko`` wins over ``ventrilo`` before the first login).
    """

    path = control.listener_log
    if path is None:
        return ""
    try:
        size = path.stat().st_size
        with path.open("r", encoding="utf-8", errors="replace") as handle:
            handle.seek(max(0, size - 20000))
            tail = handle.read()
    except OSError:
        return ""
    markers = [
        line
        for line in tail.splitlines()
        if "rbc_proxy_listener_loaded" in line
        or "rbc_proxy_listener_done" in line
        or "account_detected" in line
    ]
    if not markers:
        return ""
    last = markers[-1]
    if "rbc_proxy_listener_loaded" in last:
        return (
            " - the addon started but has not seen a login since, so it is reading another "
            "account's control file; log in again with mitmdump running"
        )
    if "rbc_proxy_listener_done" in last:
        return " - the addon is not loaded (its log ends with rbc_proxy_listener_done); restart mitmdump"
    return ""


def _queue_probe_pending(control: Control, probe: dict[str, Any], *, force: bool) -> float:
    """Publish a probe pending and wait until the addon reports it sent.

    Probes are read-only but they use the ``pending`` slot, so an in-flight
    attack would be dropped - hence the idle gate. Once published, the request
    is re-published if the addon consumes it without sending (script reload) and
    the addon's own log is watched for its "cannot send" message so a missing
    websocket fails in a second instead of after the timeout.
    """

    state = control.load()
    mode = state.get("mode")
    if state.get("running") and mode not in (None, "", "berimond"):
        raise ProbeError(f"the addon is running a {mode} session; stop it before probing")
    if isinstance(state.get("pending"), dict) and not force:
        raise ProbeError(f"control file already has pending={state['pending']!r}; use --force")
    if not force and not wait_for_idle(control, PROBE_TIMEOUT_SECONDS):
        raise ProbeError("addon is busy (pending attack or recent send); wait for it or pass --force")

    queued_at = time.time()
    pending = {**probe, "queued_at": queued_at, "request_id": 1}
    offset = log_size(control.listener_log)
    # Each action has its own "I sent it" marker: probes write last_gaa_probe, a kut
    # transfer writes last_kut. Watching the wrong one meant every transfer timed out
    # with "probe was never marked sent" even when the addon had sent it.
    kind = str(probe.get("kind"))
    marker = "last_kut" if kind == "kut_transfer" else "last_gaa_probe"
    waiting_marker = "kut_transfer_wait" if kind == "kut_transfer" else "_probe_wait"
    # A transfer can be queued right after a rejection, when the addon's own pacing
    # pause is still running (REQUEST_INTERVAL_RANGE is 20-30s), so its timeout has
    # to be longer than that pause - the earlier 25s was a coin toss.
    timeout = max(PROBE_TIMEOUT_SECONDS, TRANSFER_TIMEOUT_SECONDS) if kind == "kut_transfer" else PROBE_TIMEOUT_SECONDS
    deadline = queued_at + timeout
    republished = 0
    while time.time() < deadline:
        check_stop()
        state = control.load()
        last = state.get(marker)
        if isinstance(last, dict):
            if str(last.get("kind") or kind) == kind and float(
                last.get("sent_at", 0) or 0
            ) >= queued_at - 1:
                return queued_at
        waiting = log_since(control.listener_log, offset)
        if waiting_marker in waiting:
            raise ProbeError(
                "the addon has no open websocket to send the request on; is the game connected?"
            )
        current = state.get("pending")
        if not isinstance(current, dict) or current.get("kind") != pending.get("kind"):
            republished += 1
            if republished > WRITE_ATTEMPTS:
                raise ProbeError("the addon keeps overwriting the request; is the script reloading?")
            control.update(lambda state, pending=pending: state.__setitem__("pending", pending))
        time.sleep(CONTROL_POLL_SECONDS)
        check_stop()
    raise ProbeError(
        f"{kind} was never marked sent within {timeout:.0f}s - check the addon is loaded and the game is connected"
        + addon_binding_hint(control)
    )


def queue_probe(control: Control, *, kid: int, ax1: int, ay1: int, ax2: int, ay2: int, force: bool) -> float:
    """Queue one 13x13 ``gaa`` chunk request (needs coordinates)."""

    return _queue_probe_pending(
        control,
        {
            "kind": "gaa_probe",
            "kid": int(kid),
            "ax1": int(ax1),
            "ay1": int(ay1),
            "ax2": int(ax2),
            "ay2": int(ay2),
        },
        force=force,
    )


def queue_transfer(control: Control, *, scid: int, skid: int, units: list[list[int]], tkid: int = KID) -> float:
    """Queue a ``kut`` refill transfer through the addon."""

    return _queue_probe_pending(
        control,
        {
            "kind": "kut_transfer",
            "scid": int(scid),
            "skid": int(skid),
            "tkid": int(tkid),
            "cid": -1,
            "units": [[int(unit), int(count)] for unit, count in units],
        },
        force=False,
    )


def queue_find(control: Control, *, force: bool = False, kid: int = KID) -> float:
    """Press the game's "find target" button - an empty ``fnt`` request.

    No coordinates: the server answers with the objective it selected.
    """

    return _queue_probe_pending(control, {"kind": "fnt_probe", "kid": int(kid)}, force=force)


def find_targets(
    control: Control,
    logs_dir: Path,
    *,
    presses: int = 1,
    force: bool = False,
    gap: float = 1.5,
    timeout: float = PROBE_TIMEOUT_SECONDS,
) -> dict[str, Objective]:
    """Press "find target" ``presses`` times and collect what the game names.

    Each press returns the objective the server chose - the 2026-09-22 captures
    cycled ``1256:82 -> 1268:121 -> 1254:110 -> 1239:67`` - which is the
    coordinate-free way to learn what is still alive.
    """

    found: dict[str, Objective] = {}
    total = max(1, int(presses))
    for press in range(total):
        queued_at = queue_find(control, force=force)
        fresh: dict[str, Objective] = {}
        deadline = time.time() + timeout
        while time.time() < deadline:
            check_stop()
            fresh = fnt_targets_since(logs_dir, since=queued_at - 1)
            if fresh:
                break
            time.sleep(0.5)
        for key, objective in fresh.items():
            previous = found.get(key)
            if previous is None or objective.seen_at >= previous.seen_at:
                found[key] = objective
        detail = ", ".join(f"{key}(hp={o.hp})" for key, o in sorted(fresh.items())) or "nothing"
        print(f"find press={press + 1}/{total} target={detail} total={len(found)}")
        if press + 1 < total:
            time.sleep(max(0.0, gap))
        check_stop()
    return found


def probe_objectives(
    control: Control,
    logs_dir: Path,
    *,
    box: tuple[int, int, int, int],
    force: bool,
    timeout: float = PROBE_TIMEOUT_SECONDS,
) -> dict[str, Objective]:
    """Probe every 13x13 chunk covering ``box`` and merge the map rows.

    One chunk at a time and one clear poll between them, so a response can never
    be attributed to the wrong request.
    """

    found: dict[str, Objective] = {}
    for ax1, ay1 in chunks_for_box(*box):
        queued_at = queue_probe(
            control,
            kid=KID,
            ax1=ax1,
            ay1=ay1,
            ax2=ax1 + MAP_CHUNK_SIZE - 1,
            ay2=ay1 + MAP_CHUNK_SIZE - 1,
            force=force,
        )
        fresh: dict[str, Objective] = {}
        deadline = time.time() + timeout
        while time.time() < deadline:
            check_stop()
            fresh = objectives_since(logs_dir, since=queued_at - 1, minutes=None, limit_files=12)
            if fresh:
                break
            time.sleep(0.5)
        for key, objective in fresh.items():
            previous = found.get(key)
            if previous is None or objective.seen_at >= previous.seen_at:
                found[key] = objective
        print(
            f"probe chunk={ax1}:{ay1}-{ax1 + MAP_CHUNK_SIZE - 1}:{ay1 + MAP_CHUNK_SIZE - 1} "
            f"rows={len(fresh)} objectives_so_far={len(found)}"
        )
        time.sleep(CONTROL_POLL_SECONDS)
        check_stop()
    return found


def chunk_start(value: int) -> int:
    return max(0, int(value) - (int(value) % MAP_CHUNK_SIZE))


def chunks_for_box(x1: int, y1: int, x2: int, y2: int) -> list[tuple[int, int]]:
    xs = range(chunk_start(min(x1, x2)), chunk_start(max(x1, x2)) + 1, MAP_CHUNK_SIZE)
    ys = range(chunk_start(min(y1, y2)), chunk_start(max(y1, y2)) + 1, MAP_CHUNK_SIZE)
    return [(x, y) for y in ys for x in xs]


def parse_box(text: str) -> tuple[int, int, int, int]:
    parts = [piece.strip() for piece in text.replace(":", ",").split(",") if piece.strip()]
    if len(parts) != 4:
        raise argparse.ArgumentTypeError("expected x1,y1,x2,y2")
    try:
        x1, y1, x2, y2 = (int(piece) for piece in parts)
    except ValueError as exc:
        raise argparse.ArgumentTypeError(f"box must be integers: {exc}") from exc
    return x1, y1, x2, y2


def parse_coord(text: str) -> tuple[int, int]:
    parts = [piece.strip() for piece in text.replace(",", ":").split(":") if piece.strip()]
    if len(parts) != 2:
        raise argparse.ArgumentTypeError("expected x:y")
    try:
        return int(parts[0]), int(parts[1])
    except ValueError as exc:
        raise argparse.ArgumentTypeError(f"coordinate must be integers: {exc}") from exc


# ---------------------------------------------------------------------------
# Target cache
# ---------------------------------------------------------------------------


class TargetCache:
    def __init__(self, path: Path) -> None:
        self.path = path

    def load(self) -> dict[str, Objective]:
        try:
            data = json.loads(self.path.read_text(encoding="utf-8"))
        except (FileNotFoundError, json.JSONDecodeError):
            return {}
        if not isinstance(data, dict):
            return {}
        out: dict[str, Objective] = {}
        for key, value in (data.get("objectives") or {}).items():
            if isinstance(value, dict):
                try:
                    out[str(key)] = Objective.from_dict(value)
                except (KeyError, TypeError, ValueError):
                    continue
        return out

    def save(self, objectives: dict[str, Objective], *, source: tuple[int, int] | None, extra: dict[str, Any] | None = None) -> None:
        payload = {
            "updated_at": time.time(),
            "kingdom_id": KID,
            "source": list(source) if source else None,
            "objectives": {key: value.as_dict() for key, value in sorted(objectives.items())},
        }
        if extra:
            payload.update(extra)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(self.path.suffix + ".tmp")
        tmp.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        tmp.replace(self.path)


# ---------------------------------------------------------------------------
# Logging / account plumbing
# ---------------------------------------------------------------------------


class RunLog:
    def __init__(self, path: Path | None) -> None:
        self.path = path

    def __call__(self, message: str) -> None:
        line = f"[{datetime.now().strftime('%H:%M:%S.%f')[:-3]}] {message}"
        print(line, flush=True)
        if self.path is None:
            return
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            with self.path.open("a", encoding="utf-8") as handle:
                handle.write(line + "\n")
        except OSError:
            pass


def resolve_account(args: argparse.Namespace):
    account = accounts.account_from_args(args)
    context = runner.configure_account(account.username or None, account.aid)
    return account, context


def account_control(context) -> Control:
    listener_log = Path(context.root) / "rbc_proxy_listener.log" if context is not None else None
    if context is not None:
        return Control(Path(context.control_file), listener_log)
    return Control(Path(runner.CONTROL_FILE), Path(runner.BOT_STATE_DIR) / "rbc_proxy_listener.log")


def connect_db() -> Any | None:
    """Best-effort database handle; the discovery paths work without it.

    Every failure here is deliberately swallowed: a broken/unreachable database
    must degrade to "no data" for this script rather than abort a survey, and
    psycopg raises its own hierarchy that is not worth enumerating.
    """

    try:
        from bot.test_psql_connection import connect, read_connection_config
    except Exception as exc:  # pragma: no cover - import environment only
        print(f"db_import_failed error={exc!r}")
        return None
    try:
        conn = connect(read_connection_config())
    except Exception as exc:
        print(f"db_connect_failed error={exc!r}")
        return None
    try:
        runner.ensure_bot_tables(conn)
    except Exception as exc:
        print(f"db_ensure_tables_failed error={exc!r}")
    return conn


def learned_source(conn: Any | None, override: tuple[int, int] | None) -> tuple[int, int]:
    """The account's own Berimond camp.

    The listener learns this from ``gcl``/``jaa`` traffic and stores it per
    kingdom (``kid=10:1305:85`` on 2026-09-22), which is fresher than the
    hardcoded ``berimond.SOURCE_X/Y`` and does not need editing bot.py.
    """

    if override is not None:
        return override
    if conn is not None:
        try:
            return runner.source_for_kingdom(conn, KID)
        except Exception as exc:
            print(f"source_lookup_failed error={exc!r}")
    return int(berimond.SOURCE_X), int(berimond.SOURCE_Y)


def commander_snapshot(conn: Any | None, lids: Iterable[int]) -> dict[int, dict[str, Any]]:
    if conn is None:
        return {}
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT lord_id, available_after, status
                FROM commander_state
                WHERE aid = %s AND lord_id = ANY(%s)
                ORDER BY lord_id
                """,
                (runner.current_aid(), list(lids)),
            )
            rows = cur.fetchall()
    except Exception as exc:
        print(f"commander_query_failed error={exc!r}")
        return {}
    return {
        int(row[0]): {"available_after": int(row[1] or 0), "status": str(row[2] or "")}
        for row in rows
    }


def next_available_epoch(snapshot: dict[int, dict[str, Any]], now: float) -> float | None:
    waiting = [
        float(info["available_after"])
        for info in snapshot.values()
        if float(info["available_after"]) > now
    ]
    return min(waiting) if waiting else None


def recent_attacks(conn: Any | None, limit: int) -> list[dict[str, Any]]:
    if conn is None:
        return []
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT x_coordinate, y_coordinate, lord_id, status, return_duration,
                       coin_loot, ruby_loot, result_flag, time_created, result_received_at
                FROM attack
                WHERE aid = %s AND target_kind = 'berimond_fixed' AND kingdom_id = %s
                ORDER BY time_created DESC NULLS LAST
                LIMIT %s
                """,
                (runner.current_aid(), KID, int(limit)),
            )
            rows = cur.fetchall()
    except Exception as exc:
        print(f"attack_query_failed error={exc!r}")
        return []
    keys = (
        "x",
        "y",
        "lid",
        "status",
        "return_duration",
        "coin_loot",
        "ruby_loot",
        "result_flag",
        "created_at",
        "result_at",
    )
    return [dict(zip(keys, row)) for row in rows]


def wait_for_addon(control: Control, listener_log: Path, timeout: float) -> bool:
    """True once the addon shows any sign of life.

    The addon logs to its own file (``control_log``), so a fresh ``berimond_`` /
    ``proxy_wait`` line is proof the script is loaded and polling; a pending
    attack or a bump of ``attacks_sent`` counts too.
    """

    baseline = 0
    try:
        baseline = listener_log.stat().st_size
    except OSError:
        baseline = 0
    deadline = time.time() + max(1.0, timeout)
    markers = ("berimond_", "proxy_wait", "gaa_probe_sent", "proxy_cra_", "control_state_ignored")
    while time.time() < deadline:
        state = control.load()
        if isinstance(state.get("pending"), dict) or int(state.get("attacks_sent", 0) or 0) > 0:
            return True
        try:
            with listener_log.open("r", encoding="utf-8", errors="replace") as handle:
                handle.seek(baseline)
                fresh = handle.read()
        except OSError:
            fresh = ""
        if any(marker in fresh for marker in markers):
            return True
        time.sleep(CONTROL_POLL_SECONDS)
    return False


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------


def cmd_discover(args: argparse.Namespace) -> int:
    _, context = resolve_account(args)
    logs_dir = context.logs_dir if context is not None else Path(runner.DEFAULT_LOG_DIR)
    control = account_control(context)
    conn = None if args.no_db else connect_db()

    source = learned_source(conn, args.source)

    objectives: dict[str, Objective] = {}
    if args.from_logs:
        objectives.update(objectives_from_logs(logs_dir, minutes=args.minutes, limit_files=args.log_files))
        print(f"scanned_logs minutes={args.minutes} objectives={len(objectives)}")
    if not args.no_probe and args.find:
        objectives.update(find_targets(control, logs_dir, presses=args.find, force=args.force))
    if not args.no_probe and args.search is not None:
        objectives.update(probe_objectives(control, logs_dir, box=args.search, force=args.force))
    if not objectives:
        print("nothing found: use --find N (find-target presses), --from-logs, and/or --search x1,y1,x2,y2")

    print_objectives(objectives, source=source)
    if args.save and context is not None:
        cache = TargetCache(Path(context.root) / TARGET_FILE_NAME)
        cache.save(objectives, source=source)
        print(f"cache={cache.path}")
    if conn is not None:
        conn.close()
    return 0


def print_objectives(objectives: dict[str, Objective], *, source: tuple[int, int] | None) -> None:
    if not objectives:
        print("no Berimond objectives found")
        return
    live = [o for o in objectives.values() if o.alive]
    dead = [o for o in objectives.values() if not o.alive]
    chosen = pick_target(objectives.values(), mode="weakest", source=source)
    print(f"\nsource={source[0]}:{source[1]}" if source else "\nsource=unknown")
    print(f"alive={len(live)} defeated={len(dead)}")
    print(f"{'target':>12} {'state':>5} {'hp':>6} {'ret':>4}  links")
    for objective in sorted(objectives.values(), key=lambda o: (o.alive, o.hp, o.key)):
        state = "live" if objective.alive else "dead"
        links = " ".join(f"{x}:{y}" for x, y in objective.links) or "-"
        mark = "  <= next" if chosen is not None and objective.key == chosen.key else ""
        print(f"{objective.key:>12} {state:>5} {objective.hp:>6} {objective.return_seconds:>4}  {links}{mark}")


def cmd_status(args: argparse.Namespace) -> int:
    account, context = resolve_account(args)
    control = account_control(context)
    conn = None if args.no_db else connect_db()
    state = control.load()

    print(f"control_file={control.path}")
    print(f"account={account.username} aid={account.aid}")
    for key in ("running", "mode", "stop_reason", "attacks_sent", "max_attacks", "berimond_source", "berimond_target"):
        print(f"  {key}={state.get(key)!r}")

    lids = runner.berimond_commander_lids(args.commander_count)
    snapshot = commander_snapshot(conn, lids)
    if snapshot:
        now = time.time()
        ready = [lid for lid, info in snapshot.items() if info["available_after"] <= now]
        print(f"\ncommanders ready={len(ready)}/{len(snapshot)} lids={list(lids)}")
        for lid in sorted(snapshot):
            info = snapshot[lid]
            wait = max(0.0, info["available_after"] - now)
            print(f"  lid={lid:<3} status={info['status']:<12} ready_in={wait:7.1f}s")
    source = learned_source(conn, args.source)
    print(f"\nsource={source[0]}:{source[1]}")

    if context is not None:
        cache = TargetCache(Path(context.root) / TARGET_FILE_NAME)
        cached = cache.load()
        print(f"cached_objectives={len(cached)} file={cache.path}")
        if cached:
            print_objectives(cached, source=source)

    attacks = recent_attacks(conn, args.limit)
    if attacks:
        print(f"\nrecent berimond attacks (newest first, limit={args.limit})")
        for attack in attacks:
            print(
                f"  {attack['x']}:{attack['y']} lid={attack['lid']} status={attack['status']} "
                f"ret={attack['return_duration']} coin={attack['coin_loot']} ruby={attack['ruby_loot']} "
                f"flag={attack['result_flag']}"
            )
    if conn is not None:
        conn.close()
    return 0


def cmd_run(args: argparse.Namespace) -> int:
    account, context = resolve_account(args)
    if context is None:
        raise SystemExit("run needs a known account; pass --<username> like the other commands")
    control = account_control(context)
    log = RunLog(Path(context.root) / RUN_LOG_NAME)
    cache = TargetCache(Path(context.root) / TARGET_FILE_NAME)
    conn = None if args.no_db else connect_db()
    if conn is None:
        raise SystemExit("run needs the database (commander state + attack history); drop --no-db")

    mismatch = accounts.session_mismatch(account)
    if mismatch:
        log(f"REFUSING TO START account={account.username} reason={mismatch}")
        return 2

    lids = runner.berimond_commander_lids(args.commander_count)
    runner.ensure_commander_rows(conn, lids)
    source = learned_source(conn, args.source)
    objectives = cache.load()
    if not objectives:
        objectives = objectives_from_logs(context.logs_dir, minutes=args.minutes, limit_files=args.log_files)

    probe_error: str | None = None

    def acquire(*, full: bool) -> dict[str, Objective]:
        """Ask the game what is alive: find-target presses, then (optionally) a chunk scan.

        Returns the rows the game itself named (empty when the addon could not
        answer), so a caller that must not trust the cache can pick from those
        alone.
        """

        nonlocal probe_error
        if args.no_probe:
            return {}
        try:
            fresh = find_targets(control, context.logs_dir, presses=args.find, force=args.force)
        except ProbeError as exc:
            probe_error = str(exc)
            log(f"find_failed reason={exc}")
            fresh = {}
        else:
            probe_error = None
        if fresh:
            objectives.update(fresh)
            cache.save(objectives, source=source, extra={"last_find": time.time()})
        if not full:
            return fresh
        box = args.search or (
            source[0] - args.radius,
            source[1] - args.radius,
            source[0] + args.radius,
            source[1] + args.radius,
        )
        try:
            scanned = probe_objectives(control, context.logs_dir, box=box, force=args.force)
        except ProbeError as exc:
            log(f"chunk_probe_failed reason={exc}")
            return fresh
        if scanned:
            objectives.update(scanned)
            cache.save(objectives, source=source, extra={"last_probe": time.time()})
            fresh = {**scanned, **fresh}
        return fresh

    fresh: dict[str, Objective] = {}
    # What the cache alone would pick, kept so the lookup below can report whether
    # the game disagreed with it.
    cached_best = pick_target(objectives.values(), mode=args.pick, source=source, exclude=set())
    fallback = objectives
    if args.target is not None:
        log(f"startup_lookup_skipped reason=pinned_target target={args.target[0]}:{args.target[1]}")
    else:
        # Always start from what the game says, never from the last run's cache:
        # the camp a previous run left behind may already be defeated, and being
        # told so by the server costs a rejection and a retarget. The cache is
        # only the fallback for when the probe cannot answer (addon not loaded,
        # game not connected).
        fresh = acquire(full=args.search is not None)
        if fresh:
            log(
                f"startup_lookup live={len(fresh)} "
                + " ".join(f"{key}(hp={fresh[key].hp})" for key in sorted(fresh))
            )
        else:
            # No answer. If we could not even ask (addon not loaded, game not
            # connected), a stale cached row must not be treated as live: on
            # 2026-09-23 a day-old row sent the run straight into an ACI 203 on a
            # dead camp. Only rows seen recently may be used that way.
            if probe_error and not args.no_probe:
                cutoff = time.time() - float(args.cache_max_age)
                fresh_enough = {
                    key: objective
                    for key, objective in objectives.items()
                    if objective.alive and objective.seen_at >= cutoff
                }
                log(
                    f"startup_lookup_unavailable reason={probe_error} "
                    f"cache_fresh={len(fresh_enough)}/{len(objectives)} "
                    f"window={float(args.cache_max_age):.0f}s"
                )
                fallback = fresh_enough
                if not fresh_enough:
                    log(
                        "REFUSING TO START: the find-target lookup could not be made and no cached "
                        "camp was seen recently, so there is nothing safe to attack - restart mitmdump "
                        "(and log in) so the lookup works, or pass --target x:y / --cache-max-age 0"
                    )
                    if conn is not None:
                        conn.close()
                    return 3
            log(
                f"startup_lookup_empty cached={len(fallback)} "
                "(find-target gave nothing; attacking the best cached camp)"
            )

    target = None if args.target is None else Objective(x=args.target[0], y=args.target[1], state=0, hp=0, return_seconds=0)
    if target is None:
        # The game's own rows when it answered, the cache only as a fallback.
        target = pick_target((fresh or fallback).values(), mode=args.pick, source=source, exclude=set())
    if target is None:
        log("no live Berimond objective known; run discover first or pass --target x:y")
        return 3
    if fresh and cached_best is not None and target.key != cached_best.key:
        # The same step the failure sequence takes, applied before the first shot:
        # the game no longer offers the cached camp, so adopt its answer (the first
        # publish resets the global cooldown anyway, so this one is free).
        log(
            f"startup_retarget cached={cached_best.key} hp={cached_best.hp} -> "
            f"target={target.key} hp={target.hp} (the game no longer offers the cached camp)"
        )

    pinned = args.target is not None
    log(
        f"bot_berimond_start account={account.username} source={source[0]}:{source[1]} "
        f"target={target.key} pinned={pinned} hbw={args.hbw} lids={list(lids)} "
        f"max_attacks={args.max_attacks} pick={args.pick} probe_every={args.probe_every} "
        f"known_objectives={len(objectives)}"
    )
    # The order every run follows, so the log says what to expect after a failure.
    log(
        "berimond_sequence=lookup(always, unless --no-probe/--target)"
        f"->attack->on_fail{{verify: same_camp=army/commander(sound+backoff"
        f"{'' if args.refill else ', refill off'})|other_camp=retarget(fresh cooldown)"
        f"|aci_203=retarget|budget>{int(args.error_tolerance)}=close}}"
    )

    def publish(goal: Objective, *, reset: bool = False) -> None:
        write_intent(
            control,
            source=source,
            target=goal,
            lids=lids,
            hbw=args.hbw,
            ptt=args.ptt,
            max_attacks=args.max_attacks,
            global_cooldown=args.global_cooldown,
            max_commander_out=args.max_commander_out,
            reset=reset,
        )

    def handle_aci_rejection(stop_reason: str) -> None:
        """The server refused to START the attack (the addon stopped the transport).

        ``aci`` is the attack handshake, and ``%xt%aci%1%203%`` comes back with an
        empty body when the camp cannot be attacked any more. That is a fact about
        *that* camp, not a verdict on the run, so this asks the game where it would
        send us now: a different camp is adopted (fresh cooldown), and the same
        camp means the failure is ours (army/commander), so the alert is raised and
        the run pauses instead of hammering it. Only the error budget closes the
        run.
        """

        nonlocal target, sequence_errors, stall_reason, resume_at, resume_pending
        tolerance = int(args.error_tolerance)
        status = aci_status(stop_reason)
        sequence_errors += 1
        log(
            f"berimond_aci_rejected status={status} target={target.key} "
            f"errors={sequence_errors}/{tolerance}"
        )
        if sequence_errors > tolerance:
            play_alert(f"aci_status_{status}_budget", log)
            stall_reason = (
                f"error_budget_exhausted errors={sequence_errors} tolerance={tolerance} "
                f"class=aci status={status} target={target.key}"
            )
            log(f"berimond_error_action=close {stall_reason}")
            return
        live = verify_live_target()
        replacement = pick_target(
            live.values(), mode=args.pick, source=source, exclude=defeated | {target.key}
        )
        if replacement is not None:
            defeated.add(target.key)
            target = replacement
            reset_global_cooldown(control)
            publish(target)
            log(
                f"berimond_aci_retarget target={target.key} hp={target.hp} "
                f"status={status} cooldown=fresh"
            )
            return
        play_alert(f"aci_status_{status}", log)
        backoff = error_backoff_seconds(
            "unverified",
            army_backoff=float(args.army_backoff),
            error_pause=float(args.error_pause),
        )
        log(
            f"berimond_error_action=aci_needs_attention target={target.key} status={status} "
            f"errors={sequence_errors}/{tolerance} backoff={backoff:.0f}s"
        )
        if backoff > 0.0:
            resume_at = time.time() + backoff
            resume_pending = True

    stop = False

    def _handle_signal(signum, _frame):
        """Stop now, including whatever wait is in progress.

        Setting the flag alone left the run stuck for the rest of a probe deadline
        (a 25s wait, three presses with --find 3); ``request_stop`` makes the wait
        loops raise so the exit lands within a poll.
        """

        nonlocal stop
        first = not stop
        stop = True
        request_stop()
        if first:
            log(f"signal={signum} stopping now (aborting any wait in progress)")

    for signal_name in ("SIGINT", "SIGTERM", "SIGHUP"):
        try:
            signal.signal(getattr(signal, signal_name), _handle_signal)
        except (AttributeError, ValueError):
            pass

    # The addon does the sending, so dying without writing running=false leaves
    # the camp being attacked unattended. Ctrl-C, a kill or a closed terminal all
    # stop every account's control file (atexit covers the exit paths that the
    # handlers above do not, e.g. an exception in the supervisor itself).
    runner.install_proxy_stop_guards("bot_berimond_exit")

    defeated: set[str] = set()
    attacks_by_target: dict[str, int] = {}
    attempts_since_probe = 0
    hp_history: list[tuple[float, int]] = []
    last_attacks_sent = -1
    roster: dict[int, dict[str, Any]] = {}
    last_roster_at = 0.0
    last_busy_log = 0.0
    listener_log = Path(context.root) / "rbc_proxy_listener.log"
    error_offset = 0
    skip_streak = 0
    sequence_errors = 0
    #: Marches the addon has sent / has seen come back, counted from its log, so
    #: the refill estimate knows how many troops are still away from the camp.
    sent_count = 0
    returned_count = 0
    error_count = 0
    in_flight_count = 0
    #: True once the transfer_troops phase has queued a transfer for the current
    #: error sequence. Cleared when an attack is actually sent, so the phase runs
    #: once per troop shortage rather than once per rejection line.
    transfer_troops_ran = False
    #: The counters as they were when the current camp-stock snapshot was taken.
    #: The snapshot already includes everything that happened before it - both the
    #: marches that had left and the survivors that had landed - so the refill
    #: balance may only count what happened *since* it (the ACI answer refreshes
    #: the stock on every attack, so this baseline moves constantly).
    stock_capture_at = 0
    stock_baseline = {"sent": 0, "returned": 0, "errors": 0}
    resume_pending = False
    resume_at = 0.0
    stall_reason: str | None = None
    started_at = time.time()
    exit_reason = "complete"

    def count_marches(new_text: str) -> None:
        """Track sent / returned / rejected berimond marches from the log.

        Every attack the addon sends logs ``proxy_cra_sent ... kid=10``; every
        result it receives logs ``proxy_cat_return ... kid=10`` with the survivors.
        A rejection (``proxy_cra_error``) never became a march, so it is removed
        from the in-flight count - otherwise a run of 101s would look like an army
        that is away when it is really sitting at home.
        """

        nonlocal sent_count, returned_count, error_count, in_flight_count, stock_capture_at
        for line in new_text.splitlines():
            if "kid=10" not in line:
                continue
            if "proxy_cra_sent" in line:
                sent_count += 1
            elif "proxy_cat_return" in line:
                returned_count += 1
            elif "proxy_cra_error" in line:
                error_count += 1
        in_flight_count = max(0, sent_count - returned_count - error_count)
        captured = int((control.load().get("berimond_stock") or {}).get("at", 0) or 0)
        if captured != stock_capture_at:
            stock_capture_at = captured
            stock_baseline["sent"] = sent_count
            stock_baseline["returned"] = returned_count
            stock_baseline["errors"] = error_count

    def since_snapshot() -> tuple[int, int]:
        """``(in_flight, resolved)`` marches started after the stock snapshot."""

        sent = sent_count - stock_baseline["sent"]
        resolved = returned_count - stock_baseline["returned"]
        errors = error_count - stock_baseline["errors"]
        return max(0, sent - resolved - errors), max(0, resolved)

    def verify_live_target() -> dict[str, Objective]:
        """Ask the game which camp it would send us to right now."""

        if args.no_probe:
            return {}
        try:
            found = find_targets(control, context.logs_dir, presses=1, force=False)
        except ProbeError as exc:
            log(f"berimond_error_verify_failed reason={exc}")
            return {}
        if found:
            objectives.update(found)
        return found

    def refill_units_wanted() -> dict[int, int]:
        """``unit_id: 0`` for each unit to refill - capacities come from config.

        ``--refill-units 10,620`` or bare ``--refill-units 10``. A ``10:302`` form
        is still accepted but the count is ignored: the amount to send is measured
        (``plan_refill``), not configured.
        """

        wanted: dict[int, int] = {}
        for item in str(args.refill_units).split(","):
            text = item.strip()
            if not text:
                continue
            unit_text = text.split(":", 1)[0].strip()
            try:
                wanted[int(unit_text)] = 0
            except ValueError:
                continue
        return wanted

    def request_refill(trigger: str) -> bool:
        """TRANSFER_TROOPS phase - one measured ``kut`` transfer per troop shortage.

        Called once when the server says the army is missing (CRA 101/313 ->
        ``class=army``, or a troop-shortage skip). "Once" is enforced by
        ``transfer_troops_ran``, which is cleared as soon as an attack is actually
        sent, so a burst of rejections in one sequence cannot queue a transfer per
        rejection.

        The amount is measured, never hardcoded: the camp's last captured stock
        (``gui``/``aci``) is what is home now, the marches the addon has sent are
        subtracted because they took their troops out of the camp, the survivors
        they bring back are added (measured from ``cat`` where known, estimated
        with the measured survival ratio where not), and what is left of the
        capacity (``bot/berimond.py::TOTAL_CAPACITY``) is what is missing. Only
        ``--refill-safety`` of that gap is sent, so the camp is never overfilled
        and no march is wasted. The transfer is a march - ``RS`` 7200s in the
        capture - so the run waits for it and re-checks as it goes.

        Returns True when a transfer was queued, i.e. when the run is already
        waiting for troops to arrive.
        """

        nonlocal resume_at, resume_pending, transfer_troops_ran
        if not args.refill:
            return False
        if transfer_troops_ran:
            log(f"transfer_troops_phase=skipped trigger={trigger} reason=already_ran_this_sequence")
            return False
        log(f"transfer_troops_phase=start trigger={trigger}")
        targets = refill_units_wanted()
        if not targets:
            log("transfer_troops_phase=skipped reason=no_refill_units")
            return False
        if args.refill_castle_id is not None:
            source: tuple[int, int] | None = (int(args.refill_castle_id), int(args.refill_source_kingdom))
        else:
            live = control.load().get("last_kut")
            if isinstance(live, dict) and live.get("scid") is not None:
                source = (int(live["scid"]), int(live.get("skid", 0)))
            else:
                source = learned_refill_source(
                    context.logs_dir,
                    cache_path=Path(context.root) / TARGET_FILE_NAME.replace("targets", "refill_source"),
                )
        if source is None:
            log(
                "transfer_troops_phase=skipped reason=no_source_castle "
                "(no kut in the captures; pass --refill-castle-id)"
            )
            return False
        state = control.load()
        stock, captured = camp_stock(state)
        if not stock:
            log(
                "transfer_troops_phase=skipped reason=no_camp_stock "
                "(open the camp's troop view once so the stock is captured)"
            )
            return False
        if captured and time.time() - captured > float(args.refill_stock_max_age):
            log(
                f"transfer_troops_phase=skipped reason=stale_camp_stock "
                f"age={int(time.time() - captured)}s max={float(args.refill_stock_max_age):.0f}s"
            )
            return False
        # Do not queue a second transfer while one is still marching. The 2026-09-23
        # 08:08 move of 252 VDH was refused with `%xt%kut%1%88%` ("not enough space
        # for aux units") twelve minutes after a 286 VDH transfer had been queued
        # with RS=7200 - i.e. while it was still in the air. Troops-only moves are
        # fine on their own (144/168/278/286/314 all went through), so this is a
        # state refusal, not a bad request.
        in_flight_refill = state.get("berimond_refill")
        if isinstance(in_flight_refill, dict):
            jobs = [job for job in (in_flight_refill.get("jobs") or []) if isinstance(job, dict)]
            arrives_in = max(
                (
                    float(job.get("seconds", 0) or 0) - (time.time() - float(in_flight_refill.get("at", 0) or 0))
                    for job in jobs
                ),
                default=0.0,
            )
            if arrives_in > 0:
                log(
                    f"transfer_troops_phase=skipped reason=transfer_in_flight "
                    f"arrives_in={arrives_in:.0f}s (a second transfer would be refused)"
                )
                return False
        per_attack = army_need()
        sent_units = {unit: per_attack.get(unit, 0) for unit in targets}
        returns = [item for item in (state.get("berimond_returns") or []) if isinstance(item, dict)]
        live_in_flight, live_resolved = since_snapshot()
        ratios = survival_ratios(state, sent_units) if returned_count > 0 else {}
        now = time.time()
        estimates = {
            unit: camp_estimate(
                unit,
                sent=sent_units,
                home=stock,
                captured=captured,
                in_flight=live_in_flight,
                resolved=live_resolved,
                ratios=ratios,
                returns=returns,
                now=now,
            )
            for unit in sent_units
        }
        units = plan_refill(
            sent=sent_units,
            expected={unit: parts["total"] for unit, parts in estimates.items()},
            safety=float(args.refill_safety),
        )
        log(
            f"transfer_troops_phase=plan sent={sent_count} returned={returned_count} "
            f"errors={error_count} since_snapshot={live_in_flight}+{live_resolved} "
            f"stock_age={max(0, int(now) - captured) if captured else -1}s "
            f"safety={float(args.refill_safety):.2f} "
            + " ".join(
                f"u{unit}: home={parts['home']:.0f}+landed={parts['landed']:.0f}"
                f"-away={parts['away']:.0f}={parts['now']:.0f}now"
                f"+back={parts['back']:.0f}+out~{parts['out']:.0f}"
                f"={parts['total']:.0f}/{capacity_for(unit)}"
                for unit, parts in sorted(estimates.items())
            )
        )
        if not units:
            log(
                "transfer_troops_phase=skipped reason=camp_has_enough "
                f"in_flight={in_flight_count} returns={returned_count}"
            )
            return False
        try:
            queued_at = queue_transfer(control, scid=source[0], skid=source[1], units=units)
        except ProbeError as exc:
            log(f"transfer_troops_phase=failed reason={exc}")
            return False
        transfer_troops_ran = True
        log(
            f"transfer_troops_phase=sent scid={source[0]} skid={source[1]} "
            + " ".join(f"u{unit} +{count}" for unit, count in units)
            + f" camp_had={stock}"
        )

        # The arrival time comes back in the kut response; wait for it, otherwise
        # fall back to the configured wait. A refusal has to be spotted here too:
        # the answer is a bare status with no body (`%xt%kut%1%88%`), so without
        # this the run resumed as if the camp had been topped up. It is not quite a
        # "nothing was sent" failure - the request did go out - so the reason says
        # refused and the caller falls back to its backoff.
        wait_seconds = float(args.refill_wait)
        deadline = time.time() + 20.0
        while time.time() < deadline:
            check_stop()
            state = control.load()
            refused = state.get("berimond_refill_failed")
            if isinstance(refused, dict) and float(refused.get("at", 0) or 0) >= queued_at - 1:
                log(
                    f"transfer_troops_phase=refused status={refused.get('status')} "
                    f"requested={' '.join(f'u{unit} {count}' for unit, count in units)} "
                    "(the game refused the move - it reported no space for aux units; "
                    "the camp is unchanged and the backoff applies)"
                )
                return False
            refill = state.get("berimond_refill")
            if isinstance(refill, dict) and float(refill.get("at", 0) or 0) >= queued_at - 1:
                jobs = [job for job in (refill.get("jobs") or []) if isinstance(job, dict)]
                seconds = max((float(job.get("seconds", 0) or 0) for job in jobs), default=0.0)
                if seconds > 0 and float(args.refill_wait) <= 0.0:
                    wait_seconds = seconds
                break
            time.sleep(CONTROL_POLL_SECONDS)
        wait_seconds = min(wait_seconds, float(args.refill_max_wait))
        log(
            f"transfer_troops_phase=wait seconds={wait_seconds:.0f} "
            "(skip it in game to resume sooner; the camp is re-checked as it goes)"
        )
        resume_at = time.time() + wait_seconds
        resume_pending = True
        return True

    def handle_addon_errors(new_text: str) -> None:
        """Classify each berimond rejection the addon just logged.

        Explicit army/commander reasons are settled from the line alone; the
        ambiguous ones (status 5, or a line naming another coordinate) cost one
        find-target press so the decision comes from the game, not a guess.
        """

        nonlocal target, skip_streak, stall_reason, sequence_errors, resume_pending, resume_at
        tolerance = int(args.error_tolerance)
        for line in new_text.splitlines():
            skip = parse_cra_skip(line)
            if skip is not None:
                skip_streak += 1
                log(
                    f"berimond_skip reason={skip} streak={skip_streak} "
                    f"attacks_sent={max(0, last_attacks_sent)}"
                )
                if skip_streak >= int(args.skip_tolerance) and last_attacks_sent <= 0:
                    stall_reason = (
                        f"stalled reason={skip} skips={skip_streak} attacks_sent=0 "
                        "(the addon refused every attack; see the proxy_cra_skip reasons above)"
                    )
                if "army" in str(skip).lower() or "troop" in str(skip).lower():
                    # Out of troops in the camp - the same cure as a 101.
                    request_refill(f"troop_skip reason={skip}")
                continue
            error = parse_cra_error(line)
            if error is None:
                continue
            reason = str(error.get("reason") or "unknown")
            reported = error.get("target")
            matched = reported is None or reported == target.key
            needs_map = reason not in ARMY_ERROR_REASONS and reason not in COMMANDER_ERROR_REASONS
            live = verify_live_target() if needs_map else {}
            classification, replacement = classify_cra_error(
                error,
                believed=target.key,
                live=live,
                pick=args.pick,
                source=source,
                exclude=defeated,
            )
            log(
                f"berimond_error status={error.get('status')} reason={reason} "
                f"reported={reported or 'unknown'} believed={target.key} matched={matched} "
                f"class={classification}"
            )

            if classification == "target_changed" and replacement is not None:
                defeated.add(target.key)
                target = replacement
                publish(target)
                reset_global_cooldown(control)
                log(
                    f"berimond_error_action=retarget target={target.key} hp={target.hp} "
                    "cooldown=fresh"
                )
                continue

            sequence_errors += 1
            refill_queued = False
            if classification == "army":
                describe_camp_stock(control.load(), log)
                refill_queued = request_refill(
                    f"missing_troops status={error.get('status')} reason={reason} class={classification}"
                )
            if sequence_errors > tolerance:
                play_alert(f"cra_{reason}_{classification}_budget", log)
                stall_reason = (
                    f"error_budget_exhausted errors={sequence_errors} tolerance={tolerance} "
                    f"class={classification} reason={reason} status={error.get('status')}"
                )
                log(f"berimond_error_action=close {stall_reason}")
                continue

            if "berimond_alert_sound" not in new_text:
                play_alert(f"cra_{reason}_{classification}", log)
            backoff = error_backoff_seconds(
                classification,
                army_backoff=float(args.army_backoff),
                error_pause=float(args.error_pause),
            )
            if refill_queued:
                # Troops are already on their way, so the long army backoff would
                # only sleep on top of the transfer the run is waiting for anyway
                # (it used to overwrite that wait). Change nothing here.
                backoff = 0.0
            log(
                f"berimond_error_action={classification}_needs_attention target={target.key} "
                f"reason={reason} status={error.get('status')} lid={error.get('lid')} "
                f"errors={sequence_errors}/{tolerance} backoff={backoff:.0f}s"
                + (" waiting=refill" if refill_queued else "")
            )
            if backoff > 0.0:
                try:
                    runner.stop_proxy_transport(f"berimond_error_backoff class={classification}")
                except (OSError, ValueError) as exc:
                    log(f"berimond_error_backoff_stop_failed error={exc!r}")
                resume_at = time.time() + backoff
                resume_pending = True

    try:
        runner.clear_proxy_awaiting_db(conn)
    except Exception as exc:
        raise SystemExit(f"could not clear the stored proxy pending state: {exc!r}") from exc
    publish(target, reset=True)
    error_offset = log_size(listener_log)
    describe_camp_stock(control.load(), log)
    log(f"intent_written target={target.key} control={control.path} waiting_for_addon")
    if not wait_for_addon(control, listener_log, args.startup_timeout):
        log(
            "WARNING: the addon has not reacted yet - check that rbc_proxy_listener.py is loaded "
            "in mitmdump and holding an open websocket; continuing to wait"
        )

    try:
        while not stop:
            # ---- rejections first, so they are classified before any stop ----
            fresh_log = log_since(listener_log, error_offset)
            if fresh_log:
                error_offset += len(fresh_log.encode("utf-8", errors="replace"))
                count_marches(fresh_log)
                handle_addon_errors(fresh_log)
            if stall_reason is not None:
                exit_reason = stall_reason
                break
            if resume_pending:
                if time.time() < resume_at:
                    time.sleep(CONTROL_POLL_SECONDS)
                    continue
                resume_pending = False
                publish(target)
                log(f"berimond_error_resume target={target.key} errors={sequence_errors}")
                continue

            state = control.load()
            stop_reason = str(state.get("stop_reason") or "")
            if state.get("running") is False and stop_reason and not resume_pending:
                # The addon stops the transport on an ACI rejection, but that only
                # says THIS camp could not be started. Ask the game and retarget
                # instead of treating the run as failed after one status.
                if is_aci_rejection(stop_reason):
                    handle_aci_rejection(stop_reason)
                    if stall_reason is not None:
                        exit_reason = stall_reason
                        break
                    continue
                exit_reason = f"addon_stopped reason={stop_reason}"
                break

            attacks_sent = int(state.get("attacks_sent", 0) or 0)
            if attacks_sent != last_attacks_sent:
                if last_attacks_sent >= 0:
                    delta = attacks_sent - last_attacks_sent
                    attacks_by_target[target.key] = attacks_by_target.get(target.key, 0) + delta
                    attempts_since_probe += delta
                last_attacks_sent = attacks_sent
                skip_streak = 0
                sequence_errors = 0
                # A real attack went out, so the army is healthy again: the next
                # troop shortage gets its own transfer_troops phase.
                transfer_troops_ran = False
                log(f"attacks_sent={attacks_sent} target={target.key}")

            if attacks_sent >= int(args.max_attacks):
                exit_reason = f"max_attacks={args.max_attacks}"
                break

            # ---- target liveness: the game is the only source of truth -------
            if attempts_since_probe >= int(args.probe_every):
                attempts_since_probe = 0
                if not wait_for_idle(control, PROBE_TIMEOUT_SECONDS):
                    continue
                if not args.no_probe:
                    acquire(full=False)
                current = objectives.get(target.key)
                if current is None:
                    log(f"target_missing target={target.key} not in the newest map rows")
                elif not current.alive:
                    defeated.add(target.key)
                    log(
                        f"target_defeated target={target.key} attacks={attacks_by_target.get(target.key, 0)} "
                        f"hp={current.hp}"
                    )
                else:
                    hp_history.append((time.time(), current.hp))
                    log(
                        f"target_probe target={target.key} hp={current.hp} state={current.state} "
                        f"ret={current.return_seconds}"
                    )
                    window = [hp for _, hp in hp_history[-int(args.stall_probes):]]
                    if len(window) >= int(args.stall_probes) and len(set(window)) == 1 and current.hp > 1:
                        log(
                            f"target_stalled hp={current.hp} unchanged over {len(window)} probes; "
                            "check that the army is actually landing"
                        )

            current = objectives.get(target.key)
            target_dead = target.key in defeated or (current is not None and not current.alive)
            if target_dead and not pinned:
                replacement = pick_target(
                    objectives.values(), mode=args.pick, source=source, exclude=defeated | {target.key}
                )
                if replacement is None and args.probe_when_empty and not args.no_probe:
                    acquire(full=True)
                    replacement = pick_target(
                        objectives.values(), mode=args.pick, source=source, exclude=defeated
                    )
                if replacement is None:
                    exit_reason = f"no_live_target defeated={sorted(defeated)}"
                    break
                target = replacement
                log(f"target_rotated target={target.key} hp={target.hp} ret={target.return_seconds}")
                publish(target)

            # ---- commander allocation: wait for the earliest free lord -------
            now = time.time()
            if now - last_roster_at >= ROSTER_POLL_SECONDS:
                last_roster_at = now
                roster = commander_snapshot(conn, lids)
            if roster and all(info["available_after"] > now for info in roster.values()):
                wait = min(info["available_after"] for info in roster.values()) - now
                wait = min(wait, args.idle_sleep)
                if now - last_busy_log >= BUSY_LOG_SECONDS:
                    last_busy_log = now
                    ready = [info["available_after"] for info in roster.values()]
                    log(
                        f"commanders_busy count={len(roster)} next_free_in={wait:.1f}s "
                        f"latest_free_in={max(ready) - now:.1f}s"
                    )
                end = time.time() + max(0.5, wait)
                while time.time() < end and not stop:
                    time.sleep(min(CONTROL_POLL_SECONDS, max(0.0, end - time.time())))
                continue

            time.sleep(CONTROL_POLL_SECONDS)
    except StopRequested:
        # Ctrl-C arrived inside a probe/backoff wait. Everything is already
        # published, so just leave through the same stop path below.
        exit_reason = "interrupted"
        log("bot_berimond_interrupted (Ctrl-C during a wait; stopped before doing anything else)")
    finally:
        try:
            runner.stop_proxy_transport(f"bot_berimond_exit reason={exit_reason}")
        except (OSError, ValueError) as exc:
            log(f"stop_proxy_transport_failed error={exc!r}")
        # Any other control file still marked running is stale (only one addon
        # session can be bound), so clear it too rather than leave a loop armed.
        try:
            runner.stop_proxy_everywhere(f"bot_berimond_exit reason={exit_reason}")
        except (OSError, ValueError) as exc:
            log(f"stop_proxy_everywhere_failed error={exc!r}")

    summary = attack_summary(conn, started_at)
    log(
        f"bot_berimond_done reason={exit_reason} elapsed={time.time() - started_at:.0f}s "
        f"by_target={attacks_by_target} defeated={sorted(defeated)} summary={summary}"
    )
    if conn is not None:
        conn.close()
    if exit_reason.startswith("interrupted"):
        return 130
    if exit_reason.startswith("no_live_target"):
        return 3
    if exit_reason.startswith("stalled"):
        return 5
    if exit_reason.startswith("error_budget_exhausted"):
        return 6
    return 0


def write_intent(
    control: Control,
    *,
    source: tuple[int, int],
    target: Objective,
    lids: Iterable[int],
    hbw: int,
    ptt: int,
    max_attacks: int,
    global_cooldown: float,
    max_commander_out: int,
    reset: bool = False,
) -> None:
    """Publish (or refresh) the intent the addon acts on.

    Mirrors ``bot.prepare_berimond_proxy_transport`` but with a rotating target,
    the learned camp instead of the hardcoded one, and the captured HBW.

    ``reset`` zeroes the run-scoped counters. It must be used for the first
    write of a run and *not* for rotations: ``attacks_sent`` is what the addon
    compares against ``max_attacks``, so a leftover count from a previous run
    (the control file held 221 after a sands session) would otherwise make the
    addon answer ``max_attacks_already_reached`` before sending anything.
    """

    target_payload = {
        "id": 0,
        "kingdom_id": KID,
        "x": int(target.x),
        "y": int(target.y),
        "target_level": None,
        "task_name": "berimond_fixed",
    }
    lid_list = [int(lid) for lid in lids]

    def mutate(state: dict[str, Any]) -> None:
        if reset:
            state["attacks_sent"] = 0
            state["pending"] = None
            state["last_cra"] = None
            state["last_global_attack_sent_at"] = None
            state["cra_consecutive_errors"] = 0
            state["cra_error_timestamps"] = []
        else:
            state.setdefault("attacks_sent", 0)
        state["running"] = True
        state["transport_only"] = False
        state["mode"] = "berimond"
        state["stop_reason"] = None
        state["stopped_at"] = None
        state["max_attacks"] = int(max_attacks)
        state["berimond_target"] = target_payload
        state["berimond_source"] = {"x": int(source[0]), "y": int(source[1]), "hbw": int(hbw)}
        state["berimond_ptt"] = int(ptt)
        state["berimond_commander_lids"] = lid_list
        state["commander_count"] = max(1, len(lid_list))
        state["berimond_global_attack_cooldown_seconds"] = float(global_cooldown)
        state["berimond_max_commander_out_seconds"] = int(max_commander_out)
        state["berimond_max_cra_errors"] = int(berimond.MAX_CRA_ERRORS)
        state["berimond_cra_error_window_seconds"] = int(berimond.CRA_ERROR_WINDOW_SECONDS)
        state["berimond_alert_on_error"] = bool(berimond.ALERT_ON_ERROR)
        state["berimond_alert_sound_path"] = str(berimond.ALERT_SOUND_PATH)
        state["berimond_attack_interval_target_seconds"] = float(berimond.ADI_TO_CRA_TARGET_SECONDS)

    for _ in range(WRITE_ATTEMPTS):
        control.update(mutate)
        time.sleep(CONTROL_POLL_SECONDS)
        current = control.load()
        published = current.get("berimond_target")
        if isinstance(published, dict) and int(published.get("x", -1)) == int(target.x) and int(
            published.get("y", -1)
        ) == int(target.y):
            return
        if not control.idle():
            continue
    raise SystemExit("could not publish the target into the control file (addon keeps overwriting it)")


def attack_summary(conn: Any | None, since: float) -> dict[str, Any]:
    if conn is None:
        return {}
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT COUNT(*), COALESCE(SUM(coin_loot), 0), COALESCE(SUM(ruby_loot), 0),
                       COALESCE(AVG(return_duration), 0)
                FROM attack
                WHERE aid = %s
                  AND target_kind = 'berimond_fixed'
                  AND kingdom_id = %s
                  AND time_created >= %s
                """,
                (runner.current_aid(), KID, int(since)),
            )
            row = cur.fetchone()
    except Exception as exc:
        return {"error": repr(exc)}
    if row is None:
        return {}
    return {
        "attacks": int(row[0] or 0),
        "coin_loot": int(row[1] or 0),
        "ruby_loot": int(row[2] or 0),
        "avg_return_seconds": round(float(row[3] or 0.0), 1),
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def add_common(parser: argparse.ArgumentParser) -> None:
    """Options every subcommand shares.

    Registered on the top-level parser only (same shape as ``bot/cli.py``):
    they go *before* the subcommand, e.g. ``--ventrilo discover --from-logs``.
    Duplicating them onto the subparsers would reset them to their defaults on
    this Python version.
    """

    accounts.add_account_arguments(parser)
    parser.add_argument("--commander-count", type=int, default=berimond.COMMANDER_COUNT)
    parser.add_argument("--source", type=parse_coord, default=None, help="override the learned camp x:y")
    parser.add_argument("--no-db", action="store_true", help="skip every database lookup")
    parser.add_argument("--log-files", type=int, default=600, help="how many capture files to read")
    parser.add_argument("--minutes", type=float, default=60.0, help="only read captures newer than this")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Repeat-attack driver for the Berimond kingdom (KID 10).",
    )
    add_common(parser)
    # `required=False` on purpose: a bare invocation is easy to type by accident
    # (the shared flags come first), so it gets an explanation instead of
    # argparse's one-liner.
    subparsers = parser.add_subparsers(dest="command")

    discover = subparsers.add_parser("discover", help="list live Berimond objectives")
    discover.add_argument("--search", type=parse_box, default=None, help="x1,y1,x2,y2 area to probe")
    discover.add_argument("--find", type=int, default=1, help="find-target button presses (0 disables)")
    discover.add_argument("--from-logs", action="store_true", help="parse recent captures instead of probing")
    discover.add_argument("--no-probe", action="store_true", help="never send a gaa probe")
    discover.add_argument("--save", action="store_true", help="write the target cache")
    discover.add_argument("--force", action="store_true", help="queue the probe even if a pending exists")
    discover.set_defaults(func=cmd_discover)

    status = subparsers.add_parser("status", help="show control state, commanders and recent attacks")
    status.add_argument("--limit", type=int, default=10)
    status.set_defaults(func=cmd_status)

    run = subparsers.add_parser("run", help="attack repeatedly, rotating defeated objectives")
    run.add_argument("--target", type=parse_coord, default=None, help="pin one target x:y")
    run.add_argument(
        "--cache-max-age",
        type=float,
        default=900.0,
        help="when the find-target lookup cannot be made, only trust cached camps seen within "
        "this many seconds (0 = trust anything, i.e. the old behaviour)",
    )
    run.add_argument("--pick", choices=("weakest", "nearest", "first"), default="weakest")
    run.add_argument("--max-attacks", type=int, default=100)
    run.add_argument("--hbw", type=int, default=DEFAULT_HBW)
    run.add_argument("--ptt", type=int, default=berimond.PTT)
    run.add_argument("--global-cooldown", type=float, default=berimond.GLOBAL_ATTACK_COOLDOWN_SECONDS)
    run.add_argument("--max-commander-out", type=int, default=berimond.MAX_COMMANDER_OUT_SECONDS)
    run.add_argument("--find", type=int, default=3, help="find-target presses when hunting for a live target")
    run.add_argument("--probe-every", type=int, default=3, help="attacks between liveness probes")
    run.add_argument("--error-tolerance", type=int, default=1, help="rejections tolerated in a sequence before the run closes")
    run.add_argument("--error-pause", type=float, default=30.0, help="backoff after a commander/unverified rejection")
    run.add_argument("--army-backoff", type=float, default=180.0, help="backoff after an army rejection (troops need to come back)")
    run.add_argument("--refill", action=argparse.BooleanOptionalAction, default=True, help="top the camp up with a kut transfer after a troop error")
    run.add_argument(
        "--refill-units",
        default="10",
        help="units to top the camp up with, e.g. 10 or 10,620 (capacities come from "
        "bot/berimond.py, the amounts are measured)",
    )
    run.add_argument(
        "--refill-safety",
        type=float,
        default=0.9,
        help="fraction of the estimated missing troops to actually send (default 0.9)",
    )
    run.add_argument("--refill-castle-id", type=int, default=None, help="source castle id (default: learned from your last kut)")
    run.add_argument("--refill-source-kingdom", type=int, default=0)
    run.add_argument("--refill-wait", type=float, default=0.0, help="seconds to wait for arrival; 0 = use the game's RS")
    run.add_argument("--refill-max-wait", type=float, default=7200.0, help="cap on that wait")
    run.add_argument("--refill-stock-max-age", type=float, default=900.0, help="ignore a camp stock older than this")
    run.add_argument("--skip-tolerance", type=int, default=3, help="skips with no attack sent before the run gives up")
    run.add_argument("--radius", type=int, default=60, help="final-resort chunk scan around the learned camp")
    run.add_argument("--probe-when-empty", action="store_true", default=True)
    run.add_argument("--stall-probes", type=int, default=3, help="identical hp probes before warning")
    run.add_argument("--idle-sleep", type=float, default=120.0, help="max sleep while every lord is out")
    run.add_argument("--startup-timeout", type=float, default=40.0, help="how long to wait for the addon to react")
    run.add_argument("--search", type=parse_box, default=None, help="probe this box before starting")
    run.add_argument("--force", action="store_true")
    run.add_argument("--no-probe", action="store_true", help="never probe; rotate from cached rows only")
    run.set_defaults(func=cmd_run)
    args = parser.parse_args(accounts.apply_account_flags(list(sys.argv[1:] if argv is None else argv)))
    if not getattr(args, "command", None):
        parser.print_usage(sys.stderr)
        print(
            "bot_berimond.py: error: no subcommand given. Add discover | status | run, e.g.\n"
            "  ./venv/bin/python bot_berimond.py --ventrilo --commander-count 16 "
            "run --max-attacks 300\n"
            "(shared flags go BEFORE the subcommand)",
            file=sys.stderr,
        )
        raise SystemExit(2)
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if getattr(args, "commander_count", 0) < 1:
        raise SystemExit("--commander-count must be >= 1")
    try:
        return int(args.func(args))
    except ProbeError as exc:
        print(f"probe failed: {exc}")
        return 4


if __name__ == "__main__":
    raise SystemExit(main())
