"""Experimental GAM-only incoming movement ledger."""
from __future__ import annotations

import json
import threading
from pathlib import Path

from .protocol import parse_xt_packet


def _integer(value):
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _location(value):
    if not isinstance(value, list) or len(value) < 3:
        return {"x": None, "y": None}
    return {"x": _integer(value[1]), "y": _integer(value[2])}


def _player(players, player_id):
    if player_id is None:
        return {"name": None, "id": None, "level": None,
                "legendary_level": None, "might_points": None}
    for player in players:
        if isinstance(player, dict) and _integer(player.get("OID")) == player_id:
            return {
                "name": player.get("N"), "id": player_id,
                "level": _integer(player.get("L")),
                "legendary_level": _integer(player.get("LL")),
                "might_points": _integer(player.get("MP")),
            }
    return {"name": None, "id": player_id, "level": None,
            "legendary_level": None, "might_points": None}


def extract_composition(_movement):
    """No verified GAM formation paths are available in the observed sample."""
    return {"troops": None, "tools": None, "waves": None,
            "left": None, "middle": None, "right": None}


def is_incoming(movement, location):
    """Current candidate rule; refine here when GAM movement types are verified."""
    target = _location(movement.get("TA"))
    return (_integer(movement.get("KID")) == location["own_kid"]
            and target["x"] == location["own_x"]
            and target["y"] == location["own_y"])


class IncomingLedger:
    def __init__(self, config_path: Path):
        self.config_path = config_path
        self.lock = threading.Lock()
        self.location = None
        self.entries = {}
        try:
            saved = json.loads(config_path.read_text(encoding="utf-8"))
            self.location = self._validate(saved)
        except (OSError, ValueError, TypeError):
            pass

    @staticmethod
    def _validate(value):
        location = {key: _integer(value.get(key)) for key in ("own_kid", "own_x", "own_y")}
        if any(part is None for part in location.values()):
            raise ValueError("kingdom, X and Y must be integers")
        if location["own_kid"] < 0 or location["own_x"] < 0 or location["own_y"] < 0:
            raise ValueError("kingdom and coordinates must be non-negative")
        return location

    def configure(self, value):
        location = self._validate(value)
        with self.lock:
            if location != self.location:
                self.location = location
                self.entries.clear()
                temporary = self.config_path.with_suffix(".tmp")
                temporary.write_text(json.dumps(location) + "\n", encoding="utf-8")
                temporary.replace(self.config_path)
        return location

    def snapshot(self, now):
        with self.lock:
            records = sorted(self.entries.values(), key=lambda item: (item["arrival_at"], item["mid"]))
            return {"location": self.location, "active_count": sum(item["arrival_at"] > now for item in records),
                    "active": [item for item in records if item["arrival_at"] > now],
                    "arrived": [item for item in records if item["arrival_at"] <= now]}

    def observe(self, raw: str, received_at: float):
        packet = parse_xt_packet(raw)
        if not packet or packet.get("command") != "gam":
            return []
        payload = packet.get("payload")
        if not isinstance(payload, dict):
            return []
        movements = payload.get("M")
        players = payload.get("O")
        if not isinstance(movements, list):
            return []
        if not isinstance(players, list):
            players = []
        added = []
        with self.lock:
            location = self.location
            if location is None:
                return added
            for record in movements:
                if not isinstance(record, dict) or not isinstance(record.get("M"), dict):
                    continue
                movement = record["M"]
                mid = _integer(movement.get("MID"))
                kid = _integer(movement.get("KID"))
                pt = _integer(movement.get("PT"))
                tt = _integer(movement.get("TT"))
                source = _location(movement.get("SA"))
                target = _location(movement.get("TA"))
                if (mid is None or mid in self.entries or pt is None or tt is None
                        or not is_incoming(movement, location)):
                    continue
                source_id = _integer(movement.get("SID"))
                if source_id is None:
                    source_id = _integer(movement.get("OID"))
                target_id = _integer(movement.get("TID"))
                entry = {"mid": mid, "kid": kid, "pt": pt, "tt": tt,
                         "first_observed_at": received_at,
                         "arrival_at": received_at + (tt - pt),
                         "source": source, "target": target,
                         "sender": _player(players, source_id),
                         "receiver": _player(players, target_id),
                         "composition": extract_composition(record)}
                self.entries[mid] = entry
                added.append(entry)
        return added
