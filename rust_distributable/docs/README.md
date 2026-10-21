# OpenAuto: documentation home

OpenAuto is a Rust application that runs a Goodgame Empire account directly over the game's WebSocket (no browser, no
Python): a local service (`empire-daemon`) holds one game session per account, a Tauri window (`desktop`) shows and controls
it, and everything it knows lives in a local SQLite database. This folder is the manual; start here.

## Where to look

| I want to know | Go to |
| --- | --- |
| How the pieces fit, who owns what, why a thing is built the way it is | [`architecture/`](architecture/README.md) |
| What a packet looks like, what it means, how it was proven | [`network/`](network/README.md) (one file per command, plus captures, sequences, topics) |
| A game rule (price, level, map limits, travel options) | [`game_infrastructure/`](game_infrastructure/) |
| What is in the downloaded game files and how to look something up in them | [`game_data/`](game_data/README.md) (originals in `../data/raw/game_files/`) |
| The lookup tables: every unit, kingdom and skip item by the game's own names | [`game_data/tables.md`](game_data/tables.md) (the tables are `../data/tables/*.csv`) |
| A past bug or change: the problem, the fix, what is unverified | [`problems/`](problems/) (numbered) |
| How to run, build, clean, reset | [`architecture/running.md`](architecture/running.md), [`../utils/README.md`](../utils/README.md), [`cleanup/`](cleanup/) |

## The project, by folder

```text
crates/empire-game      static game data (troops, tools, kingdoms, travel costs) and the attack encoder
crates/empire-core      rules, pacing, planning, licence, SQLite store (the single source of truth)
crates/empire-daemon    the local API and one game session per account
crates/empire-socket    opens the game WebSocket (upgrade headers, deflate offer, inflating compressed frames) for everything that talks to the game
crates/empire-login     a standalone login proof (no daemon, database or window)
crates/empire-hunt      the command-line hunter
crates/empire-license-admin   the administrator's token issuer
desktop                 the Tauri window (src/ is the UI, src-tauri/ the shell)
admin                   licence administration notes and the private key (git-ignored)
utils                   everyday commands and the scripts behind them
data                    raw/ (downloaded game files, git-ignored), tables/ (the lookup tables built from them, committed), retired/ (old databases)
docs                    this manual
```

Layer rules (who may do what): [`architecture/modules.md`](architecture/modules.md). How changes are made:
[`architecture/working-rules.md`](architecture/working-rules.md). The project rule in `CLAUDE.md` adds that every change to the code
also updates the problem notes and the architecture notes, and stays modular.

## Conventions that keep this findable

* **Packets:** one file per command in `network/`, all in one template, raw captures in `network/captures/`
  ([`network/README.md`](network/README.md) has the rule).
* **Game facts:** a number the game decides lives in `game_infrastructure/`, with where it was read from.
* **Evidence:** every fact says whether it was *proven live*, *captured* or *read from the client only*; a guess is an open question, never a fact.
* **Problems:** each fix gets a numbered note in `problems/` saying what was wrong, how it was fixed and what is not verified.
