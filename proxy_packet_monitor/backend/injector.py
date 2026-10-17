"""Send pasted frames through the live game WebSocket, for debugging.

The page server and the proxy are separate processes, so this is a small file channel in the
data folder, like `control.json`:

* the page writes `inject.json` (`id`, `frames`, `to_client`) with [`InjectChannel.submit`] and
  waits briefly for the answer;
* the proxy addon calls [`InjectChannel.tick`] on its poll loop, sends each frame with the
  callback it is given, and writes `inject_result.json` (`id`, `sent`, `error`).

Pure Python, no mitmproxy import, so it is tested on its own.
"""
from __future__ import annotations
import json
import time
import uuid
from pathlib import Path

REQUEST = "inject.json"
RESULT = "inject_result.json"
MAX_FRAMES = 20
MAX_FRAME_CHARS = 65_536


class InjectError(Exception):
    """The frames could not be sent; the message is shown to the user."""


def split_frames(text) -> list[str]:
    """One frame per non-empty line. Surrounding whitespace is dropped; the frame itself is untouched."""
    if not isinstance(text, str):
        raise InjectError("Nothing to send.")
    frames = [line.strip() for line in text.splitlines() if line.strip()]
    if not frames:
        raise InjectError("Nothing to send.")
    if len(frames) > MAX_FRAMES:
        raise InjectError(f"At most {MAX_FRAMES} frames at a time.")
    if any(len(frame) > MAX_FRAME_CHARS for frame in frames):
        raise InjectError(f"A frame is longer than {MAX_FRAME_CHARS:,} characters.")
    return frames


def _write(path: Path, body: dict) -> None:
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(body), encoding="utf-8")
    temporary.replace(path)


def _read(path: Path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


class InjectChannel:
    def __init__(self, folder: Path):
        self.request = Path(folder) / REQUEST
        self.result = Path(folder) / RESULT
        # A request left by an earlier run must not be sent again when the proxy starts.
        previous = _read(self.request)
        self.last_id = previous.get("id") if isinstance(previous, dict) else None

    # ---- page side ----
    def submit(self, text, to_client=False, wait_s=3.0) -> dict:
        """Queue the frames and wait for the proxy's answer. Raises InjectError on a refusal."""
        frames = split_frames(text)
        request = {"id": uuid.uuid4().hex, "frames": frames, "to_client": bool(to_client), "at": time.time()}
        _write(self.request, request)
        deadline = time.monotonic() + wait_s
        while time.monotonic() < deadline:
            answer = _read(self.result)
            if isinstance(answer, dict) and answer.get("id") == request["id"]:
                if answer.get("error"):
                    raise InjectError(answer["error"])
                return {"sent": answer.get("sent", 0), "to_client": request["to_client"]}
            time.sleep(0.05)
        raise InjectError("The proxy did not answer. Start the proxy (restart it once after an update).")

    # ---- proxy side ----
    def tick(self, send) -> None:
        """Send a new request's frames with `send(frame, to_client)`, then record the outcome.

        `send` raises on failure; frames already sent stay sent and the count says how many.
        """
        request = _read(self.request)
        if not isinstance(request, dict) or request.get("id") in (None, self.last_id):
            return
        self.last_id = request["id"]
        sent, error = 0, None
        try:
            for frame in request.get("frames", []):
                send(frame, bool(request.get("to_client")))
                sent += 1
        except Exception as exc:
            error = str(exc) or exc.__class__.__name__
            if sent:
                error = f"{error} (sent {sent} before that)"
        _write(self.result, {"id": request["id"], "sent": sent, "error": error, "at": time.time()})
