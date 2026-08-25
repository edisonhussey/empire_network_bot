from __future__ import annotations

from enum import Enum
from urllib.parse import urlparse


class WebsocketType(Enum):
    OUTER_WEBSOCKET = "outer_websocket"

    @property
    def data(self) -> dict[str, str]:
        mapping = {
            "outer_websocket": {
                "name": "outer_websocket",
                "url": "wss://ep-live-temp1-game.goodgamestudios.com/",
            },
        }
        return mapping[self.value]

    @property
    def name_value(self) -> str:
        return self.data["name"]

    @property
    def url(self) -> str:
        return self.data["url"]

    @property
    def host(self) -> str:
        return urlparse(self.url).hostname or ""

    def matches_url(self, url: str) -> bool:
        host = urlparse(url).hostname or ""
        if host == self.host:
            return True
        return (
            host.endswith(".goodgamestudios.com")
            and "-game" in host
            and host.startswith("ep-live-")
        )


OUTER_WEBSOCKET = WebsocketType.OUTER_WEBSOCKET
