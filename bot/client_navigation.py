"""Narrow desktop bridge for client-local game navigation.

Opening general map view is not a protocol command: the client changes its own
screen and only then sends GBL/GAA reads.  Recruitment can therefore navigate
between castles with JCA, but the final castle -> map transition needs one real
UI action.  This module does exactly that and leaves confirmation to the
network observer.
"""

from __future__ import annotations

from dataclasses import dataclass
import subprocess
import sys


@dataclass(frozen=True)
class MapButtonTarget:
    browser: str = "Google Chrome"
    url_fragment: str = "empire.goodgamestudios.com"
    relative_x: float = 0.70
    relative_y: float = 0.965

    def __post_init__(self) -> None:
        if not 0.0 < self.relative_x < 1.0 or not 0.0 < self.relative_y < 1.0:
            raise ValueError("map button coordinates must be relative window positions")


def chrome_map_click_script(target: MapButtonTarget) -> str:
    """Return AppleScript that targets only the Chrome game window."""

    browser = target.browser.replace('"', '\\"')
    fragment = target.url_fragment.replace('"', '\\"')
    return f'''
tell application "{browser}"
    set gameBounds to missing value
    repeat with gameWindow in windows
        try
            set gameURL to URL of active tab of gameWindow
            if gameURL contains "{fragment}" then
                set gameBounds to bounds of gameWindow
                set index of gameWindow to 1
                activate
                exit repeat
            end if
        end try
    end repeat
end tell
if gameBounds is missing value then error "Goodgame Empire Chrome window not found"
delay 0.25
set leftEdge to item 1 of gameBounds
set topEdge to item 2 of gameBounds
set rightEdge to item 3 of gameBounds
set bottomEdge to item 4 of gameBounds
set clickX to leftEdge + ((rightEdge - leftEdge) * {target.relative_x})
set clickY to topEdge + ((bottomEdge - topEdge) * {target.relative_y})
tell application "System Events"
    click at {{clickX as integer, clickY as integer}}
end tell
return "clicked " & (clickX as integer) & ":" & (clickY as integer)
'''.strip()


def request_general_map(target: MapButtonTarget = MapButtonTarget()) -> str:
    """Click the game map control and return a small diagnostic string.

    The caller must still wait for genuine client GAA traffic before treating
    the transition as successful.
    """

    if sys.platform != "darwin":
        raise RuntimeError("automatic game map navigation currently requires macOS")
    completed = subprocess.run(
        ["osascript", "-e", chrome_map_click_script(target)],
        check=False,
        capture_output=True,
        text=True,
        timeout=8.0,
    )
    output = (completed.stdout or completed.stderr).strip()
    if completed.returncode != 0:
        raise RuntimeError(output or f"osascript exited {completed.returncode}")
    return output or "clicked"
