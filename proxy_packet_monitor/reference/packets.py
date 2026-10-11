"""Goodgame Empire protocol layer: constants, packet framing and pure parsers.

This module deliberately has **no** mutable runtime state, no file I/O and no
database access. It is the shared low-level vocabulary used by the proxy
listener, the capture populator and the probe scripts.

Everything here is pure: same input -> same output. Runtime/account state lives
in :mod:`bot.accounts`, :mod:`bot.db` and :mod:`bot.bot`.
"""

from __future__ import annotations

import json
import math
import re
from typing import Any

# ---------------------------------------------------------------------------
# Wire protocol constants
# ---------------------------------------------------------------------------

#: `xt` packet server header used by the outer kingdom realms.
SAND_SERVER_HEADER = "EmpireEx_21"

#: Area type that represents an RBC / baron tower. Player castles, villages and
#: fortresses use different `AI` area types, so this filter is load-bearing.
AREA_BARRON = 2

#: Burning Sands outer kingdom id.
SANDS_KID = 1

#: Storm kingdom id (overridden where the storm tables are used).
STORM_KID = 4

#: GAA probes are requested in square chunks of this many tiles per side.
MAP_CHUNK_SIZE = 13

#: Coin-travel flag used when sending a CRA packet from the sand kingdom.
HBW_VALUE = 1007

#: Level-61 RBC threshold used by the Sands task definitions.
LV61 = 61

#: Last-known default castle coordinates. These are only fallbacks; the bot
#: resolves the real castle position at login time.
DEFAULT_SOURCE_X = 593
DEFAULT_SOURCE_Y = 613

# Backwards-compatible aliases.
SOURCE_X = DEFAULT_SOURCE_X
SOURCE_Y = DEFAULT_SOURCE_Y

XT_PACKET_RE = re.compile(r"(%xt%[^\n\r]+%)")


# ---------------------------------------------------------------------------
# Framing
# ---------------------------------------------------------------------------


def xt_packet(
    command: str,
    payload: dict[str, Any],
    *,
    request_id: int = 1,
    server_header: str | None = None,
) -> str:
    """Frame an outbound `xt` packet."""

    return "%xt%{}%{}%{}%{}%".format(
        server_header or SAND_SERVER_HEADER,
        command,
        int(request_id),
        json.dumps(payload, separators=(",", ":")),
    )


def extract_raw_packet(text: str) -> str | None:
    """Pull the first `%xt%...%` packet out of a capture block."""

    match = re.search(r"(?ms)^\s*raw:\s*\n(?P<packet>%xt%[^\n]+%)\s*\n---", text)
    if match:
        return match.group("packet").strip()
    match = XT_PACKET_RE.search(text)
    if match:
        return match.group(1).strip()
    return None


def parse_xt_packet(packet: str) -> dict[str, Any] | None:
    """Parse a raw `xt` packet into its component fields.

    Returns ``None`` when the packet is not a well-formed `xt` frame. The
    optional ``status`` field is only present for server-header packets.
    """

    fields = packet.strip().strip("%").split("%")
    if len(fields) < 4 or fields[0] != "xt":
        return None

    if fields[1].startswith("EmpireEx_"):
        if len(fields) < 5:
            return None
        server_header = fields[1]
        command = fields[2]
        request_id = fields[3]
        if len(fields) >= 6 and fields[4] in {"0", "1", "2", "3", "4", "5", "256"}:
            status = fields[4]
            payload_text = fields[5]
        else:
            status = None
            payload_text = fields[4]
    else:
        server_header = None
        command = fields[1]
        request_id = fields[2]
        status = fields[3]
        payload_text = fields[4] if len(fields) >= 5 else ""

    if payload_text:
        try:
            payload: Any = json.loads(payload_text)
        except json.JSONDecodeError:
            payload = payload_text
    else:
        payload = None

    return {
        "server_header": server_header,
        "command": command,
        "request_id": request_id,
        "status": status,
        "payload": payload,
        "raw": packet,
    }


# ---------------------------------------------------------------------------
# Payload readers
# ---------------------------------------------------------------------------


def sands_level_from_gaa_value(gaa_value: int | None) -> int | None:
    """Convert a GAA `AI[4]` value into an RBC level for the sand kingdom."""

    if gaa_value is None:
        return None
    return math.floor(1.9 * math.pow(max(0, int(gaa_value)), 0.555)) + 35


def adi_level(payload: dict[str, Any]) -> int | None:
    """Read the exact target level out of an ADI response payload."""

    for row in payload.get("AE") or []:
        if isinstance(row, list) and len(row) >= 3 and row[2] == "TL":
            try:
                return int(row[0])
            except (TypeError, ValueError):
                return None
    return None


def adi_gaa(payload: dict[str, Any]) -> tuple[int | None, int | None, int | None]:
    """Read ``(x, y, gaa_value)`` from the nested ``gaa.AI`` row of an ADI payload."""

    ai = (payload.get("gaa") or {}).get("AI")
    if not isinstance(ai, list) or len(ai) < 5:
        return None, None, None
    try:
        return int(ai[1]), int(ai[2]), int(ai[4])
    except (TypeError, ValueError):
        return None, None, None


# ---------------------------------------------------------------------------
# Login / account identity
# ---------------------------------------------------------------------------

#: Command used by the login handshake.
LOGIN_COMMAND = "lli"

#: Login payload keys that are session secrets. They are never logged or
#: persisted, and are listed here so callers have one place to strip them.
LOGIN_SECRET_KEYS = ("LT", "RCT")


def login_identity(payload: Any) -> dict[str, str] | None:
    """Extract the account identity from a login payload.

    Returns ``{"name": <game account name>, "portal_account_id": <AID>}``.

    IMPORTANT: ``AID`` in the login handshake is a **shared portal account id**.
    Every game account under one login reports the *same* value, so it must not
    be used to tell accounts apart. In observed captures both accounts reported
    ``AID=1782860727866351909`` while ``NOM`` differed. ``NOM`` is the
    per-game-account name and is the only discriminator available here.

    Never returns the ``LT`` / ``RCT`` session tokens.
    """

    if not isinstance(payload, dict):
        return None
    name = payload.get("NOM") or payload.get("nom")
    if name is None:
        return None
    name = str(name).strip()
    if not name:
        return None
    aid = payload.get("AID", payload.get("aid"))
    return {
        "name": name,
        "portal_account_id": str(aid).strip() if aid is not None else "",
    }


def login_identity_from_packet(packet: str) -> dict[str, str] | None:
    """Detect a login handshake packet and return the account it belongs to."""

    parsed = parse_xt_packet(packet)
    if parsed is None or parsed.get("command") != LOGIN_COMMAND:
        return None
    return login_identity(parsed.get("payload"))


# ---------------------------------------------------------------------------
# Castles / source coordinates
# ---------------------------------------------------------------------------

#: Area types that represent a player's own castle. ``12`` is the castle in an
#: outer kingdom, ``1`` is the home castle in green.
CASTLE_AREA_TYPES = (12, 1)


def _castle_container(payload: Any) -> dict[str, Any] | None:
    """Locate the castle list inside a payload.

    Three shapes exist in the wild:
      * ``gbd`` embeds it:      ``payload["gcl"]["C"]``
      * a raw ``gcl`` response: ``payload["C"]``
      * pygge-wrapped:          ``payload["data"]["C"]``
    """

    if not isinstance(payload, dict):
        return None
    for candidate in (payload.get("gcl"), payload, payload.get("data")):
        if isinstance(candidate, dict) and isinstance(candidate.get("C"), list):
            return candidate
    return None


def castles_by_kingdom(payload: Any) -> dict[int, list[dict[str, Any]]]:
    """Parse a castle list into ``{kingdom_id: [castle, ...]}``.

    Each castle is ``{type, x, y, castle_id, name}``, read from the nested
    ``AI`` row ``[type, x, y, castle_id, ..., name]``.
    """

    container = _castle_container(payload)
    if container is None:
        return {}

    result: dict[int, list[dict[str, Any]]] = {}
    for block in container["C"]:
        if not isinstance(block, dict):
            continue
        try:
            kingdom_id = int(block.get("KID"))
        except (TypeError, ValueError):
            continue
        for area in block.get("AI") or []:
            row = area.get("AI") if isinstance(area, dict) else area
            if not isinstance(row, list) or len(row) < 4:
                continue
            try:
                castle_type = int(row[0])
                x = int(row[1])
                y = int(row[2])
                castle_id = int(row[3])
            except (TypeError, ValueError):
                continue
            entry = {
                "type": castle_type,
                "x": x,
                "y": y,
                "castle_id": castle_id,
                "name": str(row[10]) if len(row) > 10 else "",
            }
            result.setdefault(kingdom_id, []).append(entry)
    return result


def source_for_kingdom(payload: Any, kingdom_id: int) -> dict[str, Any] | None:
    """The player's own castle in one kingdom, or ``None``.

    Prefers the outer-kingdom castle type (12), then the green home castle (1).
    """

    castles = castles_by_kingdom(payload).get(int(kingdom_id)) or []
    for preferred in CASTLE_AREA_TYPES:
        for castle in castles:
            if castle["type"] == preferred:
                return castle
    return castles[0] if castles else None


def sources_by_kingdom(payload: Any) -> dict[int, dict[str, Any]]:
    """The best source castle per kingdom: ``{kingdom_id: castle}``."""

    result: dict[int, dict[str, Any]] = {}
    for kingdom_id in castles_by_kingdom(payload):
        castle = source_for_kingdom(payload, kingdom_id)
        if castle is not None:
            result[kingdom_id] = castle
    return result


def source_from_area(payload: Any) -> tuple[int, int, dict[str, Any]] | None:
    """Read the area just landed in: ``(kingdom_id, x, y, area)``.

    ``jaa``/``gca`` carry the active area as ``O.AP = [?, ?, ?, x, y, ...]``
    alongside ``KID`` -- i.e. "once I land in it".
    """

    if not isinstance(payload, dict):
        return None
    try:
        kingdom_id = int(payload.get("KID"))
    except (TypeError, ValueError):
        return None
    area = (payload.get("gca") or {}).get("O") if isinstance(payload.get("gca"), dict) else None
    ap = area.get("AP") if isinstance(area, dict) else None
    if not isinstance(ap, list) or len(ap) < 5:
        return None
    try:
        x, y = int(ap[3]), int(ap[4])
    except (TypeError, ValueError):
        return None
    area_type = area.get("AI")
    kind = int(area_type[0]) if isinstance(area_type, list) and area_type else -1
    return kingdom_id, x, y, {
        "type": kind,
        "x": x,
        "y": y,
        "castle_id": int(area.get("OID") or 0) if isinstance(area, dict) else 0,
        "name": str(area.get("N") or "") if isinstance(area, dict) else "",
    }


# ---------------------------------------------------------------------------
# Castle inventory / army checks
# ---------------------------------------------------------------------------


def inventory_from_payload(payload: Any) -> dict[int, int]:
    """Troops and tools sitting in the attacking castle: ``{id: count}``.

    An ``adi`` response carries the castle's stock in ``gui.I`` as
    ``[[id, count], ...]``. Reading it is what lets a payload be checked against
    what the account actually owns *before* sending, instead of discovering the
    shortfall from a "not enough troops" rejection after the ADI has been spent.
    """

    container = payload.get("gui") if isinstance(payload, dict) else None
    rows = container.get("I") if isinstance(container, dict) else None
    if not isinstance(rows, list):
        return {}

    inventory: dict[int, int] = {}
    for row in rows:
        if not isinstance(row, (list, tuple)) or len(row) < 2:
            continue
        try:
            inventory[int(row[0])] = int(row[1])
        except (TypeError, ValueError):
            continue
    return inventory


def army_requirements(payload: Any) -> dict[int, int]:
    """Total troops per id an attack payload asks for: ``{id: count}``.

    Sums every wave and every flank, so a 4-wave payload reports the real
    total it will try to march. Accepts either a full attack payload or the
    bare wave list from ``Attack.to_payload()``.
    """

    waves = payload.get("A") if isinstance(payload, dict) else payload
    needs: dict[int, int] = {}
    for wave in waves or []:
        if not isinstance(wave, dict):
            continue
        for side in wave.values():
            if not isinstance(side, dict):
                continue
            for key in ("T", "U"):
                for slot in side.get(key) or []:
                    if not isinstance(slot, (list, tuple)) or len(slot) < 2:
                        continue
                    try:
                        unit_id, count = int(slot[0]), int(slot[1])
                    except (TypeError, ValueError):
                        continue
                    if unit_id > 0 and count > 0:
                        needs[unit_id] = needs.get(unit_id, 0) + count
    return needs


def army_shortfall(payload: Any, inventory: dict[int, int]) -> list[tuple[int, int, int]]:
    """``(id, needed, available)`` for every unit the inventory cannot supply."""

    shortfall: list[tuple[int, int, int]] = []
    for unit_id, needed in sorted(army_requirements(payload).items()):
        available = int(inventory.get(unit_id, 0))
        if available < needed:
            shortfall.append((unit_id, needed, available))
    return shortfall


#: Most tools the game accepts per flank, per wave, relative to its troops.
#: Derived from every recorded attack in this repo: 5 ladders with 30 troops
#: (1:6) is accepted, 7 ladders with 30 troops is rejected with status 5.
TOOLS_PER_TROOP_RATIO = 6


def tool_count(payload: Any) -> int:
    """Total tools (ladders, rams, ...) an attack payload asks for."""

    waves = payload.get("A") if isinstance(payload, dict) else payload
    total = 0
    for wave_data in waves or []:
        if not isinstance(wave_data, dict):
            continue
        for flank in ("L", "M", "R"):
            side = wave_data.get(flank)
            if not isinstance(side, dict):
                continue
            total += sum(int(s[1]) for s in (side.get("T") or []) if _valid_slot(s))
    return total


def tool_limit_violations(
    payload: Any,
    *,
    troops_per_tool: int = TOOLS_PER_TROOP_RATIO,
) -> list[tuple[int, str, int, int]]:
    """Flanks asking for more tools than the game allows.

    Returns ``(wave_number, flank, tools, troops)`` for each offending flank, so
    the caller can say exactly which wave is over the limit instead of letting
    the server answer with an opaque "the action could not be performed".
    """

    waves = payload.get("A") if isinstance(payload, dict) else payload
    violations: list[tuple[int, str, int, int]] = []
    for index, wave_data in enumerate(waves or [], start=1):
        if not isinstance(wave_data, dict):
            continue
        for flank in ("L", "M", "R"):
            side = wave_data.get(flank)
            if not isinstance(side, dict):
                continue
            troops = sum(int(s[1]) for s in (side.get("U") or []) if _valid_slot(s))
            tools = sum(int(s[1]) for s in (side.get("T") or []) if _valid_slot(s))
            if tools and tools * troops_per_tool > troops:
                violations.append((index, flank, tools, troops))
    return violations


def _valid_slot(slot: Any) -> bool:
    try:
        return int(slot[0]) > 0 and int(slot[1]) > 0
    except (IndexError, TypeError, ValueError):
        return False


def commander_lids_from_payload(payload: Any) -> list[int]:
    """The account's commander roster as an ordered list of lord ids.

    Read from ``gbd.gli.C[].ID`` at login. The order is the player's visible
    commander order, so index 0 is commander number 1. LIDs differ between
    accounts, which is why this is learned per account instead of hardcoded.
    """

    if not isinstance(payload, dict):
        return []
    for container in (payload, payload.get("data")):
        if not isinstance(container, dict):
            continue
        node = container.get("gli")
        if not isinstance(node, dict) or not isinstance(node.get("C"), list):
            continue
        lids: list[int] = []
        for row in node["C"]:
            if not isinstance(row, dict):
                continue
            try:
                lid = int(row["ID"])
            except (KeyError, TypeError, ValueError):
                continue
            lids.append(lid)
        return lids
    return []


# ---------------------------------------------------------------------------
# Map chunk maths
# ---------------------------------------------------------------------------


def chunk_start(value: int) -> int:
    """Snap a coordinate down to the start of its map chunk."""

    return max(0, int(value) - (int(value) % MAP_CHUNK_SIZE))


def chunk_offsets(radius: int, *, limit: int | None = None) -> list[tuple[int, int]]:
    """Every chunk-aligned offset within ``radius`` tiles of the centre."""

    offsets: list[tuple[int, int]] = []
    for dx in range(-int(radius), int(radius) + 1, MAP_CHUNK_SIZE):
        for dy in range(-int(radius), int(radius) + 1, MAP_CHUNK_SIZE):
            if math.hypot(dx, dy) <= int(radius) + MAP_CHUNK_SIZE / 2:
                offsets.append((dx, dy))
    return offsets[:limit] if limit is not None else offsets
