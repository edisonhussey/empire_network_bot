"""The folder's `.env`: the one place that says which ports and executables the monitor uses.

Lookup order for each setting: a real environment variable (what process_monitor passes), then `.env`,
then the built-in default. Keys map to config entries in `SETTINGS`.
"""
from __future__ import annotations
import os
import re
from pathlib import Path

# .env key -> (config key, type)
SETTINGS = {
    "PPM_WEB_HOST": ("api_host", str),
    "PPM_WEB_PORT": ("api_port", int),
    "PPM_PROXY_HOST": ("proxy_host", str),
    "PPM_PROXY_PORT": ("proxy_port", int),
    "PPM_MITMDUMP": ("mitmdump", str),
    "PPM_DATA_DIR": ("data_dir", str),
    "PPM_CAPTURE_HOSTS": ("capture_hosts", str),
}
BY_CONFIG_KEY = {config_key: key for key, (config_key, _) in SETTINGS.items()}


def read_env(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        return values
    for line in lines:
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        values[key.strip()] = value
    return values


def overrides(path: Path) -> dict:
    """Config entries set by the environment or `.env`, already converted to their type."""
    merged = {**read_env(path), **{key: os.environ[key] for key in SETTINGS if os.environ.get(key)}}
    result = {}
    for key, (config_key, convert) in SETTINGS.items():
        value = merged.get(key, "")
        if value == "":
            continue
        try:
            result[config_key] = convert(value)
        except ValueError:
            raise ValueError(f"{key} in {path.name} must be {convert.__name__}, got {value!r}") from None
    return result


def update_env(path: Path, changes: dict[str, str]) -> None:
    """Set KEY=value lines in place, keeping comments and order; append keys that are not there yet."""
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        lines = []
    pending = dict(changes)
    for index, line in enumerate(lines):
        match = re.match(r"\s*([A-Za-z_][A-Za-z0-9_]*)\s*=", line)
        if match and match.group(1) in pending:
            lines[index] = f"{match.group(1)}={pending.pop(match.group(1))}"
    lines.extend(f"{key}={value}" for key, value in pending.items())
    temp = path.with_suffix(".tmp")
    temp.write_text("\n".join(lines) + "\n", encoding="utf-8")
    temp.replace(path)
