from __future__ import annotations

import configparser
import re
from pathlib import Path

import psycopg


HERE = Path(__file__).resolve().parent
DEFAULT_CONFIG = HERE / "psql_connect.ini"


def read_connection_config(path: Path = DEFAULT_CONFIG) -> dict[str, str]:
    text = path.read_text(encoding="utf-8")

    parser = configparser.ConfigParser()
    try:
        parser.read_string("[database]\n" + text if "[" not in text else text)
    except configparser.Error:
        parser = configparser.ConfigParser()
    if parser.has_section("database"):
        section = parser["database"]
        config = {
            "dbname": section.get("dbname") or section.get("database") or section.get("database name"),
            "user": section.get("user") or section.get("username"),
            "password": section.get("password"),
            "host": section.get("host"),
            "port": section.get("port"),
        }
        config = {key: value.strip() for key, value in config.items() if value and value.strip()}
        if config.get("dbname") and config.get("user"):
            return config

    config: dict[str, str] = {}
    patterns = {
        "user": r"username\s+(.+)",
        "password": r"password\s+(?:would be\s+)?(.+)",
        "dbname": r"database name\s*:\s*(.+)",
        "host": r"host\s*:\s*(.+)",
        "port": r"port\s*:\s*(.+)",
    }
    for key, pattern in patterns.items():
        match = re.search(pattern, text, flags=re.IGNORECASE)
        if match:
            config[key] = match.group(1).strip()

    if "dbname" not in config:
        config["dbname"] = "empire_bot"
    return config


def connect(config: dict[str, str]) -> psycopg.Connection:
    try:
        return psycopg.connect(**config)
    except psycopg.OperationalError:
        local_config = dict(config)
        local_config.pop("password", None)
        return psycopg.connect(**local_config)


def main() -> int:
    config = read_connection_config()
    safe_config = {key: ("***" if key == "password" else value) for key, value in config.items()}
    print(f"using config: {safe_config}")

    with connect(config) as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT current_database(), current_user, version()")
            database, user, version = cur.fetchone()
            print(f"connected database={database} user={user}")
            print(version.splitlines()[0])

            cur.execute(
                """
                SELECT table_name
                FROM information_schema.tables
                WHERE table_schema = 'public'
                  AND table_name IN ('rbc', 'attack')
                ORDER BY table_name
                """
            )
            tables = [row[0] for row in cur.fetchall()]
            print(f"tables found: {tables}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
