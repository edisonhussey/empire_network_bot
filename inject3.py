from __future__ import annotations

import asyncio
import json
import queue
import pprint
from mitmproxy import command, ctx, http
from datetime import datetime
import os
import sys
import threading
import time

CAPTURE_FOLDER = "captures"
os.makedirs(CAPTURE_FOLDER, exist_ok=True)
CONTROL_LOG = os.path.join(CAPTURE_FOLDER, "inject3_control.log")
SEND_FILE = os.path.join(CAPTURE_FOLDER, "inject3_send.txt")

current_file = None
last_dump = datetime.now()
active_flow = None
send_queue = queue.Queue()
inject_tasks = set()
terminal_thread_started = False
send_file_offset = 0


def control_log(message: str) -> None:
    ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")[:-3]
    with open(CONTROL_LOG, "a", encoding="utf-8") as handle:
        handle.write(f"[{ts}] {message}\n")
        handle.flush()


def packet_preview(packet: str, limit: int = 150) -> str:
    return f"{packet[:limit]}{'...' if len(packet) > limit else ''}"


def normalize_packet(line: str) -> str | None:
    packet = line.strip()
    if not packet.startswith("%xt%"):
        return None
    final_percent = packet.rfind("%")
    if final_percent <= 0:
        return None
    return packet[: final_percent + 1]


def pretty_json(value: str) -> str | None:
    try:
        parsed = json.loads(value)
    except json.JSONDecodeError:
        return None
    return json.dumps(parsed, indent=2, sort_keys=True, ensure_ascii=False)


def pretty_xt_packet(packet: str) -> str | None:
    if not packet.startswith("%xt%"):
        return None

    fields = packet.strip().strip("%").split("%")
    if not fields or fields[0] != "xt":
        return None

    if len(fields) >= 5 and fields[1].startswith("EmpireEx_"):
        labels = ["type", "server_header", "command", "request_id", "payload"]
    elif len(fields) >= 5:
        labels = ["type", "command", "request_id", "status", "payload"]
    else:
        labels = ["type"]

    lines = ["XT PACKET", "  fields:"]
    for index, field in enumerate(fields):
        label = labels[index] if index < len(labels) else f"field_{index}"
        lines.append(f"    {label}: {field}")

    if fields:
        payload = fields[-1]
        formatted_payload = pretty_json(payload)
        if formatted_payload is not None:
            lines.extend(["", "  json_payload:", formatted_payload])
        elif payload:
            lines.extend(["", "  payload:", pprint.pformat(payload, width=100)])

    lines.extend(["", "  raw:", packet])
    return "\n".join(lines)


def format_message_for_log(message: str) -> str:
    pretty_xt = pretty_xt_packet(message)
    if pretty_xt is not None:
        return pretty_xt

    pretty = pretty_json(message)
    if pretty is not None:
        return pretty

    return pprint.pformat(message, width=120) if "\n" not in message else message


def is_open_websocket(flow: http.HTTPFlow | None) -> bool:
    return (
        flow is not None
        and flow.websocket is not None
        and flow.websocket.timestamp_end is None
    )


def inject_packet(flow: http.HTTPFlow, packet: str) -> None:
    # For mitmproxy 12.x this boolean is to_client; False sends to the server.
    ctx.master.commands.call("inject.websocket", flow, False, packet.encode("utf-8"))


async def queued_sender():
    global active_flow

    while True:
        try:
            packet = send_queue.get_nowait()
        except queue.Empty:
            await asyncio.sleep(0.05)
            continue

        flow = active_flow
        if not is_open_websocket(flow):
            control_log(f"not_sent reason=no_active_websocket packet={packet_preview(packet)!r}")
            continue

        try:
            inject_packet(flow, packet)
            control_log(
                f"injected direction=client_to_server url={flow.request.url} "
                f"packet={packet_preview(packet)!r}"
            )
        except Exception as exc:
            control_log(f"inject_failed error={exc!r} packet={packet_preview(packet)!r}")


def queue_packet(line: str, *, source: str) -> None:
    if line.startswith(":gge.send "):
        line = line[len(":gge.send "):].strip()
    packet = normalize_packet(line)
    if packet is None:
        control_log(f"ignored source={source} reason=not_xt_packet line={packet_preview(line)!r}")
        return
    send_queue.put(packet)
    control_log(f"queued source={source} packet={packet_preview(packet)!r}")


async def send_file_watcher() -> None:
    global send_file_offset

    open(SEND_FILE, "a", encoding="utf-8").close()
    send_file_offset = os.path.getsize(SEND_FILE)
    control_log(f"send_file_ready path={SEND_FILE}")

    while True:
        try:
            size = os.path.getsize(SEND_FILE)
            if size < send_file_offset:
                send_file_offset = 0
            if size > send_file_offset:
                with open(SEND_FILE, "r", encoding="utf-8") as handle:
                    handle.seek(send_file_offset)
                    lines = handle.readlines()
                    send_file_offset = handle.tell()
                for line in lines:
                    line = line.strip()
                    if line:
                        queue_packet(line, source="send_file")
        except Exception as exc:
            control_log(f"send_file_error error={exc!r}")
        await asyncio.sleep(0.25)


def start_sender_task() -> None:
    for coro in (queued_sender(), send_file_watcher()):
        task = asyncio.create_task(coro)
        inject_tasks.add(task)
        task.add_done_callback(inject_tasks.discard)


def handle_websocket_message(flow: http.HTTPFlow):
    global active_flow, current_file, last_dump

    active_flow = flow
    
    msg = flow.websocket.messages[-1]
    direction = "CLIENT -> SERVER" if msg.from_client else "SERVER -> CLIENT"
    ts = datetime.now()
    
    content = msg.content
    decoded = None
    if msg.is_text:
        decoded = content.decode('utf-8', errors='replace')
    else:
        try:
            decoded = content.decode('utf-8', errors='replace')
        except:
            decoded = content.hex()
    
    if current_file is None or (ts - last_dump).total_seconds() >= 10:
        if current_file:
            current_file.close()
        filename = f"{CAPTURE_FOLDER}/gge_{ts.strftime('%Y%m%d_%H%M%S')}.log"
        current_file = open(filename, "a", encoding="utf-8")
        last_dump = ts
        control_log(f"capture_file path={filename}")
    
    formatted = format_message_for_log(decoded)
    entry = f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction}\n{formatted}\n---\n"
    current_file.write(entry)
    current_file.flush()
    
def handle_websocket_start(flow: http.HTTPFlow):
    global active_flow

    active_flow = flow
    control_log(f"websocket_start url={flow.request.url}")
    if not inject_tasks:
        start_sender_task()

def handle_websocket_end(flow: http.HTTPFlow):
    global active_flow

    if flow is active_flow:
        active_flow = None
    control_log(f"websocket_end url={flow.request.url}")

def done():
    if current_file:
        current_file.close()

# Live Sender
def terminal_sender():
    control_log("terminal_sender_started")
    
    while True:
        try:
            line = input().strip()
            if line:
                queue_packet(line, source="stdin")
        except (EOFError, KeyboardInterrupt):
            break
        except Exception as exc:
            control_log(f"sender_error error={exc!r}")
            break


def start_terminal_thread() -> None:
    global terminal_thread_started

    if terminal_thread_started:
        return
    terminal_thread_started = True
    threading.Thread(target=terminal_sender, daemon=True).start()


class GgeInjector:
    def load(self, loader) -> None:
        start_terminal_thread()
        control_log("inject3_loaded")

    @command.command("gge.send")
    def gge_send(self, packet: str) -> None:
        """Queue a raw %xt% packet for client-to-server WebSocket injection."""
        queue_packet(packet, source="mitmproxy_command")

    def websocket_message(self, flow: http.HTTPFlow) -> None:
        handle_websocket_message(flow)

    def websocket_start(self, flow: http.HTTPFlow) -> None:
        handle_websocket_start(flow)

    def websocket_end(self, flow: http.HTTPFlow) -> None:
        handle_websocket_end(flow)

    def done(self) -> None:
        done()


addons = [GgeInjector()]
