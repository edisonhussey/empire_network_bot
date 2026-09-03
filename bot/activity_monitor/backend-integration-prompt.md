# Prompt for Codex — build the local API that feeds `timeline.html`

Paste everything below into Codex as-is.

---

I have a static frontend file called `timeline.html` (Apple-style commander
activity timeline). It renders itself and needs **nothing** from you except
a local HTTP JSON endpoint. Your job is to build that endpoint on top of an
existing Postgres database, running on localhost. Do not modify
`timeline.html`.

## 1. What the frontend expects

It sends:

```
GET http://localhost:8000/api/timeline
```

(no auth, no body — plain GET). It may also send `?since=<cursor>` if we
turn on incremental mode later; for now, always return the **full**
snapshot for the day being viewed, every time it's called. The frontend
polls this endpoint every 4 seconds on its own — you don't need to push
anything.

It requires this exact JSON shape back, with a `200` status and
`Content-Type: application/json`:

```json
{
  "date": "2026-05-11",
  "commanders": [
    {
      "id": "com-1",
      "label": "COM 1",
      "events": [
        {
          "id": "evt-8841",
          "start_time": "2026-05-11T00:02:19Z",
          "duration_seconds": 1339,
          "label": "22:19",
          "icon": null
        }
      ]
    }
  ],
  "server_time": "2026-05-11T00:20:03Z"
}
```

Field rules:
- `date` — the day being displayed, `YYYY-MM-DD`. If omitted, the frontend
  derives it from the earliest event, but prefer sending it explicitly.
- `commanders[].id` — a stable string key per commander/bucket. Our source
  data does **not** have a formal numeric commander id — it's grouped by a
  plain bucket label (e.g. `"COM 1"`, `"COM 2"`, ...). That's fine: just
  use the bucket label itself as the id (slugify it if you want something
  URL-safer, e.g. `com-1`), as long as it's the *same string* on every
  call for the same bucket. Don't regenerate ids per request.
- `commanders[].label` — what's shown in the UI. Can be identical to `id`.
- `events[].id` — stable per-row key so the frontend can diff on future
  incremental polls. Prefer a real primary key from Postgres. If there
  isn't one, derive a deterministic id, e.g. `` `${commander_id}-${start_time}` ``
  — never a random/uuid-per-request value, or the UI will think every row
  changed on every poll.
- `events[].start_time` — ISO 8601 timestamp, timezone-aware. Send UTC
  (`Z` suffix) — let the browser localize.
- `events[].duration_seconds` — **numeric seconds**, drives the bar's
  width. This must be a real number even if the display label is a
  formatted string like `"22:19"` or something non-standard like
  `"123:961"` — don't try to parse the display label for width, compute
  seconds independently from the underlying start/return timestamps or
  duration column.
- `events[].label` — whatever text should print on the bar. Can be a
  formatted duration, a return-time string, an id, anything — printed
  verbatim, not parsed.
- `events[].icon` — omit or send `null` for the default icon. (Optional
  future use: an emoji string or an image URL.)
- `server_time` — current server clock in ISO 8601. Used later for
  incremental polling; harmless to include now.
- If there are no events for a commander, still return the commander with
  `"events": []` rather than omitting it — that's what produces the empty
  grey strip in the UI (this is expected/normal, not an error state).
- If there's no data at all, return `{"commanders": []}` — also expected
  and handled gracefully by the frontend.

## 2. What I need you to build

1. A small local HTTP server (pick whichever stack is already in this
   repo / most idiomatic — Node + Express, or Python + FastAPI, are both
   fine) exposing `GET /api/timeline` on `http://localhost:8000`.
2. A Postgres query behind it. Assume a table roughly like:

   ```sql
   -- adjust to match whatever the real schema turns out to be —
   -- inspect it first with \d and confirm column names before wiring this up
   commander_bucket   text        -- e.g. 'COM 1'; NOT a foreign-key id, just a label
   event_id           bigint / uuid   -- primary key, may or may not exist
   start_time         timestamptz
   duration_seconds   integer     -- or derive from (return_time - start_time)
   return_label       text        -- the raw display string, if stored separately
   event_date         date        -- or derive from start_time
   ```

   Write the actual introspection step first (`\d+ <table>` or an
   information_schema query) rather than assuming column names — adapt
   the query below once you've confirmed the real schema.

3. Query pattern (adjust names):

   ```sql
   SELECT
     commander_bucket,
     event_id,
     start_time,
     duration_seconds,
     return_label
   FROM commander_events
   WHERE event_date = $1          -- the requested day, default = current date
   ORDER BY commander_bucket, start_time;
   ```

   Support an optional `?date=YYYY-MM-DD` query param on the endpoint so
   the frontend (or curl) can request a specific day; default to today's
   date server-side if it's omitted.

4. In the server code, group the flat rows by `commander_bucket` into the
   nested `commanders[]` array shape from section 1, and serialize
   timestamps to ISO 8601 UTC.

5. **CORS**: `timeline.html` will usually be opened as a local file
   (`file://...`) or served from a different port than the API. Set:

   ```
   Access-Control-Allow-Origin: *
   ```

   (or restrict to the exact origin if you know it) on the `/api/timeline`
   response, including for `OPTIONS` preflight if your framework requires
   it explicitly.

6. **Performance**: add an index on whatever column drives the `WHERE`
   clause (e.g. `event_date` or `start_time`) if one doesn't already
   exist, since this query re-runs every 4 seconds while the dashboard is
   open:

   ```sql
   CREATE INDEX IF NOT EXISTS idx_commander_events_date
     ON commander_events (event_date, commander_bucket, start_time);
   ```

7. **Connection config**: read Postgres connection info from environment
   variables (`PGHOST`, `PGPORT`, `PGDATABASE`, `PGUSER`, `PGPASSWORD`, or
   a single `DATABASE_URL`) — don't hardcode credentials. Use a small
   connection pool, not a fresh connection per request.

8. Handle the empty-result case cleanly: if the query returns zero rows,
   respond with `{"date": "...", "commanders": [], "server_time": "..."}`
   and a `200`, not a `404` or `500` — this is a normal, expected state
   for the frontend.

9. Log each request's row count and query duration to stdout so it's easy
   to eyeball whether the polling loop is behaving once the dashboard is
   pointed at it.

## 3. Nice-to-haves if there's time (not required for v1)

- A `?since=<ISO timestamp>` mode that returns only commanders/events
  touched after that time, in this shape, so the frontend can switch to
  `mergeData()` instead of a full replace every poll:

  ```json
  { "commanders": [ /* only changed buckets, each with only new/changed events */ ],
    "removed_event_ids": ["evt-123"],
    "server_time": "2026-05-11T00:20:07Z" }
  ```

- Swap polling for a `WebSocket` or Server-Sent-Events stream at
  `/api/timeline/stream` if the data changes faster than every few
  seconds — same JSON payload shape per message, the frontend's
  `TimelineApp.mergeData(payload)` is already written to accept it, we'd
  just need a few lines added to `timeline.html` to open the socket
  instead of/alongside polling.

## 4. How I'll test it

```bash
curl -s http://localhost:8000/api/timeline | jq .
curl -s "http://localhost:8000/api/timeline?date=2026-05-11" | jq .
```

Then open `timeline.html` directly in a browser (or serve it from any
static server) — it already points at `http://localhost:8000/api/timeline`
by default (see the `CONFIG.dataSource` block near the top of the
`<script>` tag) and polls it every 4 seconds. If the shape above is
correct, bars should appear without touching the frontend file at all.
