# OpenAuto lab

A local workbench for the parts of this system that normally need a compile, a
bundle, an install and a login before anything can be seen.

It is deliberately **not** part of the Rust. It runs from the repository with
Python's standard library only, so it costs nothing to start and nothing to
change.

## Run it

```sh
cd development/lab
python3 server.py            # http://127.0.0.1:8799
```

Then open <http://127.0.0.1:8799>. Stop it with ctrl-c.

| flag | default | meaning |
|---|---|---|
| `--daemon` | `http://127.0.0.1:47821` | where the running service is |
| `--port` | `8799` | panel port |
| `--rate` | `2` | requests allowed per window |
| `--window` | `2.0` | window in seconds |

## The two rules, and why they are here

**1. An allowlist, not a proxy.** Only the daemon paths named in `ALLOWED_GET`
and `ALLOWED_POST` are forwarded. A typo shows up as `403` listing what *is*
allowed, instead of a `404` that reads like a missing feature. Adding a
reachable path is a deliberate edit in `server.py`.

**2. A hard rate ceiling: two requests per two seconds.** It applies to reads as
well as writes, and it *waits* rather than failing. That is the point of the
tool — a slow, deliberate loop. Measured: five requests take 4.02s.

The server binds to loopback only. There is no flag to change that, because the
thing it forwards to controls a live game session.

## What it is for

### The window ladder

The panel sends one `gaa` for a chosen span and then reports what came back,
computed from the stored reply rather than summarised:

```
asked 62×62 at 632,632 → returned 62×62 spanning x 632..693, 765 objects — honoured in full
```

This settles the open question about how wide a probe window may be. The spans
offered (`42, 62, 101, 140, 218`) are the ones that change the request count per
kingdom, so the ladder doubles as a cost measurement.

**A note on reading it.** The extent of the returned objects is a *floor* on the
window, never a ceiling. A reply with nothing at its edge has not been clipped,
it is just empty. The only true test of clipping is whether fortresses appear
more than one lattice step from the window origin — which is what
`simulate.py` checks.

### Raw packets

Anything in the daemon's client framing can be injected:

```
%xt%<server-header>%<command>%1%{json}%
```

The daemon parses it before queueing, so a malformed packet is refused rather
than sent. Injections need a connected session; `health` reports
`transport_connected`, and the panel will say so plainly if it is false.

**Caution: an injected reply can be mistaken for the runner's own.** A `gaa`
reply does not echo the window it answers, and the runner matches an in-flight
map scan on `KID` alone. So a hand-sent window for the kingdom the bot is
currently walking can satisfy the bot's pending request — it then marks a window
learned that it never asked for. Cosmetic while observing, but do not inject
windows for the kingdom under active initialisation if you want the walk left
alone. See D7 in `development/notes/open-defects.md`.

### The packet log with a command filter

Reads `/v1/messages` and filters by command and direction in the browser. The
daemon only serves the most recent messages, so if you need a record of
something, copy it out — see below.

## The offline simulator

```sh
python3 simulate.py                 # analyse the recorded traffic
python3 simulate.py --plan 62       # cost of a 62-wide window
python3 simulate.py --watch 0.5     # replay it slowly, one request at a time
```

It reads the `network_message` table and reports:

1. how far out replies actually reached, per window size;
2. how many lattice slots each window size answers, by geometry;
3. whether any *wide* request has ever been recorded — and if not, it says the
   wide-window claim is **unproven** rather than guessing;
4. the request cost per kingdom at a given span.

It cannot tell you whether a span that has never been sent would be honoured.
Only a live probe can, which is what the ladder is for.

## Where the logs go

`development/logs/lab-YYYYMMDD.log` — one line per forwarded request, one line
per refusal, and a line whenever the gate makes a caller wait. The console and
the file carry the same lines.

## Keeping this useful

- When a question about the live server comes up, write down **what the answer
  looked like** in `development/notes/`, not just the conclusion. The evidence is
  what makes the next person's decision cheap.
- If you add a daemon route, add it to the allowlist and say so in the notes.
- If `fortress.rs` constants change, `simulate.py` has mirrored literals that
  must change with them. They are duplicated on purpose: a mismatch surfaces as
  a coverage failure instead of as silent agreement.
