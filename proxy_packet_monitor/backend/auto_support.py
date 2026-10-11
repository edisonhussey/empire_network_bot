"""Deadline support proof of concept for the standalone proxy.

Only the mitmproxy process sends packets. The web process writes configuration;
this state machine consumes observed frames and never blocks a WebSocket callback.
"""
from __future__ import annotations

import json
import sqlite3
import time
from pathlib import Path

from .protocol import parse_xt_packet

KNOWN_TROOP_IDS = frozenset(json.loads(
    Path(__file__).with_name("known_troop_ids.json").read_text(encoding="utf-8")))


class SupportNotSent(RuntimeError):
    """Local preflight failed before WebSocket injection was attempted."""


def nonnegative(value):
    if isinstance(value, bool):
        raise ValueError("coordinates, kingdoms and LID must be integers")
    number = int(value)
    if number < 0:
        raise ValueError("coordinates, kingdoms and LID must be non-negative")
    return number


def validate_config(value):
    castles = []
    seen = set()
    for item in value.get("castles", []):
        castle = {key: nonnegative(item[key]) for key in ("kid", "x", "y")}
        identity = tuple(castle.values())
        if identity in seen:
            raise ValueError("duplicate source castle")
        seen.add(identity)
        castles.append(castle)
    destinations = {}
    for kid, point in value.get("destinations", {}).items():
        key = str(nonnegative(kid))
        if key in destinations:
            raise ValueError("duplicate kingdom destination")
        destinations[key] = {axis: nonnegative(point[axis]) for axis in ("x", "y")}
    enabled = value.get("enabled", False)
    if not isinstance(enabled, bool):
        raise ValueError("enabled must be true or false")
    if enabled and (not castles or any(str(c["kid"]) not in destinations for c in castles)):
        raise ValueError("each source castle needs a destination in its kingdom")
    return {"enabled": enabled, "castles": castles, "destinations": destinations,
            "lid": nonnegative(value.get("lid", 0))}


def save_config(path: Path, value):
    config = validate_config(value)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)
    return config


def load_config(path: Path):
    try:
        return validate_config(json.loads(path.read_text(encoding="utf-8")))
    except (OSError, ValueError, KeyError, TypeError):
        return {"enabled": False, "castles": [], "destinations": {}, "lid": 0}


def troops_from_dcl(payload, kid, castle_id):
    """AC pairs in the matching C[KID].AI[AID]; preserve all positive IDs.

    DCL does not label a row as troop or tool. The support page's gui.I is used
    later to intersect the result with actual sendable home units.
    """
    for kingdom in payload.get("C", []):
        if not isinstance(kingdom, dict) or kingdom.get("KID") != kid:
            continue
        for area in kingdom.get("AI", []):
            if isinstance(area, dict) and area.get("AID") == castle_id:
                return {int(row[0]): int(row[1]) for row in area.get("AC", [])
                        if isinstance(row, list) and len(row) >= 2
                        and isinstance(row[0], int) and isinstance(row[1], int)
                        and row[0] > 0 and row[1] > 0}
    return None


def select_troops(dcl_stock, sdi_payload):
    home = sdi_payload.get("gui", {}).get("I")
    if not isinstance(home, list):
        return []
    stock = {}
    for row in home:
        if isinstance(row, list) and len(row) >= 2 and isinstance(row[0], int) and isinstance(row[1], int):
            unit, count = row[:2]
            if unit in KNOWN_TROOP_IDS and unit in dcl_stock and count > 0:
                stock[unit] = min(count, dcl_stock[unit])
    return [[unit, count] for unit, count in
            sorted(stock.items(), key=lambda item: (-item[1], item[0]))[:10]]


def packet(header, command, payload):
    return f"%xt%{header}%{command}%1%{json.dumps(payload, separators=(',', ':'))}%"


def opposing_alliances(movement, players):
    """Resolve GAM O[] by player ID; return alliance IDs only when different.

    A missing player or AID leaves the movement unclassified. -1 is an
    observed no-alliance sentinel and is compared literally for this policy.
    """
    sender_id = movement.get("SID")
    if not isinstance(sender_id, int):
        sender_id = movement.get("OID")
    receiver_id = movement.get("TID")
    if not isinstance(sender_id, int) or not isinstance(receiver_id, int) or sender_id == receiver_id:
        return None
    by_id = {person.get("OID"): person for person in players
             if isinstance(person, dict) and isinstance(person.get("OID"), int)}
    sender, receiver = by_id.get(sender_id), by_id.get(receiver_id)
    if not sender or not receiver:
        return None
    sender_alliance, receiver_alliance = sender.get("AID"), receiver.get("AID")
    if (not isinstance(sender_alliance, int) or not isinstance(receiver_alliance, int)
            or sender_alliance == receiver_alliance):
        return None
    return sender_alliance, receiver_alliance


class AutoSupport:
    def __init__(self, config_path: Path, status_path: Path, sessions_path: Path | None = None):
        self.config_path = config_path
        self.status_path = status_path
        self.sessions_path = (sessions_path or status_path.parent / "sessions").resolve()
        self.config = load_config(config_path)
        self.config_mtime = None
        self.jobs = {}
        self.active_mid = None
        self.last_event = "Idle"
        self._recover_pending_marches()
        self._reconcile_recorded_recalls()

    def _recover_pending_marches(self):
        """Keep confirmed support MIDs available for recall after a proxy restart."""
        try:
            saved = json.loads(self.status_path.read_text(encoding="utf-8"))
        except (OSError, ValueError, TypeError):
            return
        if not isinstance(saved, dict) or not isinstance(saved.get("jobs"), list):
            return
        for item in saved["jobs"]:
            if not isinstance(item, dict):
                continue
            mid, sent_mid, state = item.get("mid"), item.get("sent_mid"), item.get("state")
            if not isinstance(mid, int):
                continue
            if state in ("sent", "await_mcm", "uncertain_mcm") and isinstance(sent_mid, int):
                item["state"] = "uncertain_mcm" if state == "await_mcm" else state
                self.jobs[mid] = item
            elif state == "recall_failed" and isinstance(sent_mid, int):
                self.jobs[mid] = item
            elif state == "recalled" and isinstance(item.get("return_at"), (int, float)):
                if time.time() < item["return_at"]:
                    self.jobs[mid] = item
            elif state in ("await_cds", "uncertain_cds"):
                item["state"] = "uncertain_cds"
                self.jobs[mid] = item
                self.active_mid = mid

    def _reconcile_recorded_recalls(self):
        """A manual MCM may have succeeded while this proxy was not observing it."""
        pending = {job["sent_mid"]: job for job in self.jobs.values()
                   if job["state"] in ("sent", "uncertain_mcm")
                   and isinstance(job.get("sent_mid"), int)}
        if not pending:
            return
        latest = {}
        for path in self.sessions_path.glob("*.sqlite3"):
            try:
                with sqlite3.connect(path.as_uri() + "?mode=ro", uri=True) as conn:
                    rows = conn.execute("SELECT received_at, raw FROM events "
                                        "WHERE direction='received' AND lower(type)='mcm'")
                    for received_at, raw in rows:
                        parsed = parse_xt_packet(raw)
                        if not parsed or parsed["status"] != "0":
                            continue
                        answer = parsed["payload"]
                        movement = answer.get("A", {}).get("M") if isinstance(answer, dict) and isinstance(answer.get("A"), dict) else None
                        support_mid = movement.get("MID") if isinstance(movement, dict) else None
                        job = pending.get(support_mid)
                        if (job and received_at >= job.get("confirmed_at", 0)
                                and received_at > latest.get(support_mid, (0, None))[0]):
                            latest[support_mid] = (received_at, movement)
            except (OSError, sqlite3.Error):
                continue
        for support_mid, (received_at, movement) in latest.items():
            job = pending[support_mid]
            job["state"] = "recalled"
            job["recall_confirmed_at"] = received_at
            pt, tt = movement.get("PT"), movement.get("TT")
            if isinstance(pt, int) and isinstance(tt, int) and tt >= pt:
                job["return_at"] = received_at + tt - pt
            job.pop("error", None)
            self.last_event = f"MID {job['mid']} recorded support recall reconciled"
        if latest:
            self.publish()

    def reload(self):
        try:
            mtime = self.config_path.stat().st_mtime_ns
        except OSError:
            mtime = None
        if mtime == self.config_mtime:
            return
        self.config_mtime = mtime
        self.config = load_config(self.config_path)
        if not self.config["enabled"]:
            for job in self.jobs.values():
                if job["state"] not in ("sent", "recalled", "recall_failed", "await_mcm",
                                        "uncertain_mcm", "failed", "cancelled",
                                        "await_cds", "uncertain_cds"):
                    job["state"] = "cancelled"
            if self.active_mid is not None and self.jobs[self.active_mid]["state"] == "cancelled":
                self.active_mid = None
        self.publish()

    def publish(self):
        self.status_path.parent.mkdir(parents=True, exist_ok=True)
        current = sorted(self.jobs.values(), key=lambda job: (job["landing"], job["mid"]))
        body = {"enabled": self.config["enabled"], "last_event": self.last_event,
                "jobs": [{key: value for key, value in job.items()
                          if key not in ("stock", "sdi")} for job in current]}
        temporary = self.status_path.with_suffix(".tmp")
        temporary.write_text(json.dumps(body) + "\n", encoding="utf-8")
        temporary.replace(self.status_path)

    def observe(self, raw, received_at=None):
        self.reload()
        parsed = parse_xt_packet(raw)
        if not parsed:
            return
        command = parsed["command"]
        data = parsed["payload"]
        now = received_at or time.time()
        if command == "mrm" and parsed["status"] == "0" and isinstance(data, dict):
            job = self.jobs.get(data.get("MID"))
            if job:
                if job["state"] in ("sent", "await_cds", "uncertain_cds"):
                    job["recall_due"] = now
                    job["recall_reason"] = "attack retreated"
                elif job["state"] not in ("recalled", "await_mcm", "uncertain_mcm",
                                           "recall_failed", "failed", "cancelled"):
                    job["state"] = "cancelled"
                    if self.active_mid == job["mid"]:
                        self.active_mid = None
                self.last_event = f"MID {job['mid']} attack retreated"
                self.publish()
            return
        if command == "gam" and isinstance(data, dict) and self.config["enabled"]:
            players = data.get("O")
            if not isinstance(players, list):
                return
            for record in data.get("M", []):
                movement = record.get("M") if isinstance(record, dict) else None
                if not isinstance(movement, dict):
                    continue
                mid, kid, target = movement.get("MID"), movement.get("KID"), movement.get("TA")
                if not isinstance(mid, int) or mid in self.jobs or not isinstance(target, list) or len(target) < 4:
                    continue
                if any(job.get("sent_mid") == mid for job in self.jobs.values()):
                    continue
                if not any((c["kid"], c["x"], c["y"]) == (kid, target[1], target[2])
                           for c in self.config["castles"]):
                    continue
                alliances = opposing_alliances(movement, players)
                if alliances is None:
                    continue
                if str(kid) not in self.config["destinations"]:
                    continue
                pt, tt = movement.get("PT"), movement.get("TT")
                if not isinstance(pt, int) or not isinstance(tt, int):
                    continue
                landing = now + max(0, tt - pt)
                self.jobs[mid] = {"mid": mid, "kid": kid, "sx": target[1], "sy": target[2],
                                  "castle_id": target[3], "landing": landing, "state": "waiting",
                                  "sender_id": movement.get("SID"),
                                  "receiver_id": movement.get("TID"),
                                  "sender_alliance_id": alliances[0],
                                  "receiver_alliance_id": alliances[1],
                                  "destination": self.config["destinations"][str(kid)].copy(),
                                  "lid": self.config["lid"]}
                self.last_event = f"MID {mid} scheduled"
                self.publish()
            return
        if self.active_mid is None or not isinstance(data, dict):
            if command == "mcm":
                self._observe_recall(parsed, now)
            return
        job = self.jobs[self.active_mid]
        if parsed["status"] != "0":
            if command == "mcm":
                self._observe_recall(parsed, now)
                return
            if command == {"await_dcl": "dcl", "await_sdi": "sdi", "await_cds": "cds",
                           "uncertain_cds": "cds"}.get(job["state"]):
                self.fail(job, f"{command} refused: {parsed['status']}")
            return
        if command == "dcl" and job["state"] == "await_dcl":
            if data.get("PID") == job.get("sender_id"):
                self.fail(job, "movement belongs to this account")
                return
            if data.get("PID") != job.get("receiver_id"):
                self.fail(job, "DCL player does not own the targeted castle")
                return
            stock = troops_from_dcl(data, job["kid"], job["castle_id"])
            if stock is None:
                self.fail(job, "DCL has no matching source castle AID")
            else:
                job["stock"] = stock
                job["state"] = "ready_sdi"
                self.publish()
        elif command == "sdi" and job["state"] == "await_sdi":
            if data.get("SCID") != job["castle_id"]:
                self.fail(job, "SDI source castle mismatch")
            else:
                chosen = select_troops(job["stock"], data)
                if not chosen:
                    self.fail(job, "no verified home units to send")
                else:
                    job["units"] = chosen
                    job["state"] = "ready_cds"
                    self.publish()
        elif command == "cds" and job["state"] in ("await_cds", "uncertain_cds"):
            answer = data.get("A", {})
            movement = answer.get("M", {}) if isinstance(answer, dict) else {}
            source = movement.get("SA") if isinstance(movement, dict) else None
            target = movement.get("TA") if isinstance(movement, dict) else None
            if (not isinstance(movement, dict) or movement.get("KID") != job["kid"]
                    or not isinstance(source, list) or len(source) < 4 or source[3] != job["castle_id"]
                    or not isinstance(target, list) or len(target) < 3
                    or target[1:3] != [job["destination"]["x"], job["destination"]["y"]]):
                self.fail(job, "CDS acknowledgement did not match source and destination")
            elif "units" in job and answer.get("A") != job["units"]:
                self.fail(job, "CDS acknowledgement did not match selected troops")
            elif isinstance(movement.get("MID"), int):
                job["state"] = "sent"
                job["sent_mid"] = movement.get("MID")
                job["confirmed_at"] = now
                self.active_mid = None
                self.last_event = f"MID {job['mid']} support confirmed"
                self.publish()
            else:
                job["state"] = "recall_failed"
                job["error"] = "CDS acknowledged without a support MID; manual recall required"
                self.active_mid = None
                self.last_event = f"MID {job['mid']}: support MID missing"
                self.publish()
        elif command == "mcm":
            self._observe_recall(parsed, now)

    def _observe_recall(self, parsed, now):
        data = parsed["payload"]
        movement = data.get("A", {}).get("M") if isinstance(data, dict) and isinstance(data.get("A"), dict) else None
        support_mid = movement.get("MID") if isinstance(movement, dict) else None
        if parsed["status"] == "0" and isinstance(support_mid, int):
            job = next((item for item in self.jobs.values()
                        if item.get("sent_mid") == support_mid
                        and item["state"] in ("sent", "await_mcm", "uncertain_mcm")), None)
            if job is None:
                return
            job["state"] = "recalled"
            job["recall_confirmed_at"] = now
            pt, tt = movement.get("PT"), movement.get("TT")
            if isinstance(pt, int) and isinstance(tt, int) and tt >= pt:
                job["return_at"] = now + tt - pt
            self.last_event = f"MID {job['mid']} support recall confirmed"
            self.publish()
        elif parsed["status"] != "0":
            pending = [item for item in self.jobs.values() if item["state"] == "await_mcm"]
            if len(pending) == 1:
                job = pending[0]
                job["state"] = "recall_failed"
                job["error"] = f"MCM refused: {parsed['status']}"
                self.last_event = f"MID {job['mid']} recall refused"
                self.publish()

    def fail(self, job, reason):
        job["state"] = "failed"
        job["error"] = reason
        self.active_mid = None
        self.last_event = f"MID {job['mid']}: {reason}"
        self.publish()

    def connection_lost(self):
        for job in self.jobs.values():
            if job["state"] == "await_cds":
                job["state"] = "uncertain_cds"
                job["error"] = "WebSocket closed after CDS injection; outcome uncertain, LID held"
            elif job["state"] == "await_mcm":
                job["state"] = "uncertain_mcm"
                job["error"] = "WebSocket closed after MCM injection; recall outcome uncertain"
            elif job["state"] not in ("sent", "recalled", "recall_failed", "uncertain_mcm",
                                      "failed", "cancelled", "uncertain_cds"):
                job["state"] = "failed"
                job["error"] = "game WebSocket closed before support dispatch"
        if self.active_mid is not None and self.jobs[self.active_mid]["state"] != "uncertain_cds":
            self.active_mid = None
        self.last_event = "Game WebSocket closed"
        self.publish()

    def tick(self, send, connected=True):
        self.reload()
        now = time.time()
        for mid, job in list(self.jobs.items()):
            if (job["state"] in ("failed", "cancelled") and now - job["landing"] > 3600
                    or job["state"] == "recalled" and "return_at" in job
                    and now - max(job["landing"], job["return_at"]) > 3600):
                del self.jobs[mid]
        for job in self.jobs.values():
            if now >= job["landing"] and job["state"] in ("sent", "await_cds", "uncertain_cds"):
                job.setdefault("recall_due", job["landing"])
                job.setdefault("recall_reason", "attack landing reached")
        if not connected:
            return
        # An injected CDS may still produce an acknowledgement and its support MID.
        # Wait for that MID; never send an uncorrelated or duplicate recall.
        pending = sorted((j for j in self.jobs.values()
                          if j["state"] == "sent" and "recall_due" in j
                          and now >= j.get("retry_at", 0)),
                         key=lambda j: (j["recall_due"], j["mid"]))
        if pending:
            job = pending[0]
            try:
                send("mcm", {"MID": job["sent_mid"]})
            except SupportNotSent as exc:
                job["retry_at"] = now + 0.5
                job["error"] = f"MCM waiting for connection: {exc}"
                self.last_event = f"MID {job['mid']}: recall waiting for connection"
                self.publish()
                return
            except Exception as exc:
                job["state"] = "uncertain_mcm"
                job["error"] = f"MCM injection outcome uncertain: {exc}"
            else:
                job["state"] = "await_mcm"
                job["recall_requested_at"] = now
                job.pop("error", None)
            self.last_event = f"MID {job['mid']}: support recall requested"
            self.publish()
            return
        for job in self.jobs.values():
            if job["state"] == "await_mcm" and now - job.get("recall_requested_at", now) > 2.0:
                job["state"] = "uncertain_mcm"
                job["error"] = "MCM acknowledgement timeout; recall outcome uncertain"
                self.last_event = f"MID {job['mid']}: recall outcome uncertain"
                self.publish()
        if not self.config["enabled"]:
            return
        if self.active_mid is None:
            candidates = sorted((j for j in self.jobs.values() if j["state"] == "waiting"),
                                key=lambda j: (j["landing"], j["mid"]))
            for job in candidates:
                if now >= job["landing"] - 7:
                    if now >= job["landing"] - 5:
                        self.fail(job, "too late to request DCL before the five-second window")
                        continue
                    if any(other["mid"] != job["mid"] and other.get("lid") == job.get("lid")
                           and (other["state"] in ("await_cds", "uncertain_cds", "sent",
                                                   "await_mcm", "uncertain_mcm", "recall_failed")
                                or (other["state"] == "recalled" and now < other.get("return_at", float("inf"))))
                           for other in self.jobs.values()):
                        self.fail(job, "support LID still occupied by another march")
                        continue
                    self.active_mid = job["mid"]
                    break
        if self.active_mid is None:
            return
        job = self.jobs[self.active_mid]
        if job["state"] == "uncertain_cds":
            return
        if now >= job["landing"]:
            if job["state"] == "await_cds":
                job["state"] = "uncertain_cds"
                job["error"] = "estimated landing passed before CDS confirmation; LID held"
                self.last_event = f"MID {job['mid']}: CDS outcome uncertain"
                self.publish()
            else:
                self.fail(job, "estimated landing passed")
            return
        if job["state"].startswith("await_"):
            if now - job.get("requested_at", now) > min(2.0, job["landing"] - now):
                if job["state"] == "await_cds":
                    job["state"] = "uncertain_cds"
                    job["error"] = "CDS acknowledgement timeout; outcome uncertain, LID held"
                    self.last_event = f"MID {job['mid']}: CDS outcome uncertain"
                    self.publish()
                else:
                    self.fail(job, "response timeout")
            return
        command = None
        payload = None
        if job["state"] == "waiting":
            command, payload = "dcl", {"CD": 1}
        elif job["state"] == "ready_sdi" and now >= job["landing"] - 5:
            command, payload = "sdi", {"TX": job["destination"]["x"], "TY": job["destination"]["y"],
                                       "SX": job["sx"], "SY": job["sy"]}
        elif job["state"] == "ready_cds":
            command, payload = "cds", {"SID": job["castle_id"], "TX": job["destination"]["x"],
                                       "TY": job["destination"]["y"], "LID": job["lid"],
                                       "WT": 1, "HBW": -1, "BPC": 0, "PTT": 0, "SD": 0, "A": job["units"]}
        if command is None:
            return
        if now < job.get("retry_at", 0):
            return
        try:
            send(command, payload)
        except SupportNotSent as exc:
            job["retry_at"] = now + 0.5
            job["error"] = f"{command} waiting for connection: {exc}"
            self.last_event = f"MID {job['mid']}: {command} waiting for connection"
            self.publish()
            return
        except Exception as exc:
            self.fail(job, f"{command} injection failed: {exc}")
            return
        job.pop("error", None)
        job["state"] = f"await_{command}"
        job["requested_at"] = now
        self.last_event = f"MID {job['mid']}: {command} sent"
        self.publish()
