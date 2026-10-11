"""Small, dependency-free copy of the listener's inbound XT decoding contract."""
from __future__ import annotations
import base64
import json
import zlib
from urllib.parse import urlparse


def matches_game_url(url: str) -> bool:
    host = urlparse(url).hostname or ""
    return host == "ep-live-temp1-game.goodgamestudios.com" or (host.startswith("ep-live-") and "-game" in host and host.endswith(".goodgamestudios.com"))


class Decoder:
    def __init__(self) -> None:
        self.streams: dict[str, zlib.Decompress] = {}

    def decode(self, data: bytes, flow_id: str, is_text: bool) -> str:
        try:
            return data.decode("utf-8")
        except UnicodeDecodeError:
            pass
        for wbits in (zlib.MAX_WBITS, -zlib.MAX_WBITS):
            try:
                return zlib.decompress(data, wbits).decode("utf-8")
            except (zlib.error, UnicodeDecodeError):
                pass
        if data.endswith(b"\x00\x00\xff\xff"):
            stream = self.streams.setdefault(flow_id, zlib.decompressobj())
            try:
                return stream.decompress(data).decode("utf-8")
            except (zlib.error, UnicodeDecodeError):
                self.streams.pop(flow_id, None)
        return f'{"TEXT" if is_text else "BINARY"} FRAME; base64: {base64.b64encode(data).decode("ascii")}'

    def forget(self, flow_id: str) -> None:
        self.streams.pop(flow_id, None)


def parse_xt_packet(packet: str) -> dict | None:
    # Matches bot.packets.parse_xt_packet, including the header/status variant.
    fields = packet.strip().strip("%").split("%")
    if len(fields) < 4 or fields[0] != "xt":
        return None
    if fields[1].startswith("EmpireEx_"):
        if len(fields) < 5:
            return None
        header, command, request_id = fields[1:4]
        if len(fields) >= 6 and fields[4] in {"0", "1", "2", "3", "4", "5", "256"}:
            status, payload_text = fields[4:6]
        else:
            status, payload_text = None, fields[4]
    else:
        header, command, request_id, status = None, fields[1], fields[2], fields[3]
        payload_text = fields[4] if len(fields) >= 5 else ""
    try:
        payload = json.loads(payload_text) if payload_text else None
    except json.JSONDecodeError:
        payload = payload_text
    return {"server_header": header, "command": command, "request_id": request_id,
            "status": status, "payload": payload, "raw": packet}


def summary(parsed: dict | None) -> str:
    if not parsed:
        return "Unparsed server message"
    payload = parsed["payload"]
    if isinstance(payload, dict):
        parts = []
        for key in ("KID", "AID", "LID", "MID", "CID", "SID", "N", "AN"):
            if key in payload and not isinstance(payload[key], (dict, list)):
                parts.append(f"{key} {payload[key]}")
        if parts:
            return " · ".join(parts[:4])
        return " · ".join(f"{key} [{len(value)}]" if isinstance(value, (list, dict)) else f"{key} {str(value)[:30]}" for key, value in list(payload.items())[:3])
    return str(payload)[:90] if payload is not None else "No payload"
