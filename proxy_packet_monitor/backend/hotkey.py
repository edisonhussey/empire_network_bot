"""The global recording shortcut: one key, anywhere in the system, while the proxy is running.

A thin, testable wrapper over pynput's `GlobalHotKeys`:

* `normalise("space")` turns friendly names into pynput's spelling (`<space>`, `<ctrl>+<alt>+r`), so Settings can say `space`.
* `Hotkey.start()` / `stop()` follow the proxy: the listener exists only while the proxy does, so the key does nothing
  (and nothing listens to the keyboard) when the monitor is idle.
* A short cooldown ignores key-repeat, so holding the key flips recording once, not dozens of times.

The key is **listened to, not swallowed**: it still reaches the application you are in. A bare letter or the space bar therefore
also types, and toggles recording every time it is pressed; pick a key you do not type (`<f9>`, `<ctrl>+<alt>+r`) if that is a problem.
On macOS the program that launched the monitor needs *Input Monitoring* (System Settings, Privacy and Security) or no key arrives.
"""
from __future__ import annotations
import time
from typing import Callable

NAMED_KEYS = {"space", "enter", "return", "tab", "esc", "escape", "backspace", "delete", "home", "end", "page_up", "page_down",
              "up", "down", "left", "right", "caps_lock", "ctrl", "alt", "cmd", "shift", "option", "command"} | {f"f{n}" for n in range(1, 21)}
ALIASES = {"escape": "esc", "return": "enter", "option": "alt", "command": "cmd"}
COOLDOWN_S = 0.4


def normalise(shortcut: str) -> str:
    """`space` -> `<space>`; `ctrl+alt+r` -> `<ctrl>+<alt>+r`; anything already in pynput's spelling is kept."""
    parts = []
    for raw in str(shortcut or "").split("+"):
        token = raw.strip()
        if not token:
            continue
        key = token.strip("<>").lower()
        parts.append(f"<{ALIASES.get(key, key)}>" if key in NAMED_KEYS else token)
    return "+".join(parts)


def _pynput_listener(mapping):
    from pynput import keyboard
    return keyboard.GlobalHotKeys(mapping)


class Hotkey:
    def __init__(self, on_press: Callable[[], None], factory=_pynput_listener, clock=time.monotonic, cooldown_s: float = COOLDOWN_S):
        self._on_press = on_press
        self._factory = factory
        self._clock = clock
        self._cooldown = cooldown_s
        self._listener = None
        self._last = None
        self.shortcut = ""
        self.error: str | None = None

    @property
    def active(self) -> bool:
        return self._listener is not None

    def start(self, shortcut: str) -> None:
        """Listen for `shortcut`. Does nothing if it is empty or already listening; failures are kept in `error`."""
        self.stop()
        self.shortcut = normalise(shortcut)
        self.error = None
        if not self.shortcut:
            return
        try:
            listener = self._factory({self.shortcut: self._fire})
            listener.start()
            self._listener = listener
        except Exception as exc:  # the dependency, a bad key name, or missing permission
            self.error = f"Global shortcut unavailable: {exc}"

    def stop(self) -> None:
        listener, self._listener = self._listener, None
        if listener is not None:
            try:
                listener.stop()
            except Exception:
                pass

    def _fire(self) -> None:
        now = self._clock()
        if self._last is not None and now - self._last < self._cooldown:
            return
        self._last = now
        self._on_press()
