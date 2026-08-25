from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parent
LOG_DIRS = [ROOT / "bot" / "logs", ROOT / "bot"]
PACKET_RE = re.compile(r"%xt%[^\\r\\n]*?%")
SECRET_KEYS = {"LT", "RCT"}


def parse_xt(packet: str) -> dict | None:
    fields = packet.strip().strip("%").split("%")
    if len(fields) < 5 or fields[0] != "xt":
        return None
    if fields[1].startswith("EmpireEx_"):
        header = fields[1]
        command = fields[2]
        request_id = fields[3]
        payload_text = fields[4] if len(fields) == 5 else fields[-1]
    else:
        header = None
        command = fields[1]
        request_id = fields[2]
        payload_text = fields[3] if len(fields) >= 4 else ""
    try:
        payload = json.loads(payload_text)
    except json.JSONDecodeError:
        return None
    return {"header": header, "command": command, "request_id": request_id, "payload": payload}


def redact(payload: dict) -> dict:
    clean = {}
    for key, value in payload.items():
        if key in SECRET_KEYS:
            clean[key] = f"<redacted len={len(str(value))}>"
        else:
            clean[key] = value
    return clean


def find_ventrilo_aid_objects(value):
    matches = []
    if isinstance(value, dict):
        text = json.dumps(value, ensure_ascii=False).lower()
        if "aid" in {str(key).lower() for key in value.keys()} and "ventrilo" in text:
            matches.append(value)
        for child in value.values():
            matches.extend(find_ventrilo_aid_objects(child))
    elif isinstance(value, list):
        for child in value:
            matches.extend(find_ventrilo_aid_objects(child))
    return matches


def iter_logs():
    paths = []
    for directory in LOG_DIRS:
        if directory.is_dir():
            paths.extend(path for path in directory.glob("*.log") if path.is_file())
    return sorted(paths, key=lambda path: path.stat().st_mtime, reverse=True)


def main() -> int:
    latest_any = None
    latest_ventrilo = None
    latest_ventrilo_object = None
    scanned = 0
    for path in iter_logs():
        scanned += 1
        try:
            lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        for lineno in range(len(lines), 0, -1):
            line = lines[lineno - 1]
            if "AID" not in line:
                continue
            for packet in PACKET_RE.findall(line):
                parsed = parse_xt(packet)
                if not parsed:
                    continue
                payload = parsed["payload"]
                if not isinstance(payload, dict):
                    continue
                if latest_ventrilo_object is None and "ventrilo" in line.lower():
                    objects = find_ventrilo_aid_objects(payload)
                    if objects:
                        latest_ventrilo_object = {
                            "file": str(path.relative_to(ROOT)),
                            "mtime": int(path.stat().st_mtime),
                            "line": lineno,
                            "command": parsed["command"],
                            "server_header": parsed["header"],
                            "request_id": parsed["request_id"],
                            "matches": [redact(obj) for obj in objects[:5]],
                        }
                if parsed["command"] != "lli" or "AID" not in payload:
                    continue
                hit = {
                    "file": str(path.relative_to(ROOT)),
                    "mtime": int(path.stat().st_mtime),
                    "line": lineno,
                    "server_header": parsed["header"],
                    "request_id": parsed["request_id"],
                    "payload": redact(payload),
                }
                if latest_any is None:
                    latest_any = hit
                text = json.dumps(payload, ensure_ascii=False).lower()
                if "ventrilo" in text:
                    latest_ventrilo = hit
                    break
            if latest_ventrilo is not None:
                break
        if latest_ventrilo is not None:
            break

    print(f"scanned_logs={scanned}")
    if latest_ventrilo:
        print("latest_ventrilo_lli=" + json.dumps(latest_ventrilo, indent=2, sort_keys=True))
        return 0
    if latest_ventrilo_object:
        print("latest_ventrilo_aid_object=" + json.dumps(latest_ventrilo_object, indent=2, sort_keys=True))
        return 0
    if latest_any:
        print("latest_any_lli=" + json.dumps(latest_any, indent=2, sort_keys=True))
        return 0
    print("no_lli_login_packet_with_AID_found")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
