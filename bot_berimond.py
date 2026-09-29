"""Compatibility launcher for the Berimond Kingdom controller."""

from bot.event.berimond_kingdom.controller import *  # noqa: F401,F403
from bot.event.berimond_kingdom.controller import main


if __name__ == "__main__":
    raise SystemExit(main())
