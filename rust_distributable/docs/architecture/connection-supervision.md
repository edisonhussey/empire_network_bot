# Connection supervision

How a dropped game connection is detected and recovered.

`direct::run` supervises `run_inner`, which is one socket session. It was
previously fire-and-forget: a dropped connection ended the run silently. On
7 Oct the Mac's Wi-Fi lost its address mid-run (`.110`, back as `.101` 90 s
later), the socket was reset, and the bot sat dead for 50+ minutes.

* Retried: a socket error, a reset, or 90 s without any frame from the server
  (`ConnectionLost`). **Exactly two retries**: the first about 1 minute after the
  loss and the second about 5 minutes after that, each with +/-20 % random jitter.
  Then it gives up (`connection.gave_up`) and the operator decides. The learned
  map is reused, so a successful retry resumes attacking instead of rescanning,
  and the running mode resumes because it is persisted.
* A session that did not stay up 2 minutes does not refill the retry budget, so a
  server that keeps dropping us is not hammered. A session that did stay up gets a
  fresh two retries.
* Final, never retried: licence refusal, **a refused login** (any non-zero `lli`), and a clean server close
  (which may be the same account logging in elsewhere; the bot does not fight it).
* While reconnecting the status reports `bot_state = "reconnecting"`.
* Marches that land while offline never produce a `cat`, so their ledger rows
  stay `sent` with no loot. They are not reconciled afterwards.
* A refused login is remembered. Status 27 carries `RS`, the seconds until a fixed
  expiry; it is stored as `login.locked_until_ms.<account>` and every connect,
  including after a restart, refuses locally until it passes. Before this existed,
  the silence watchdog turned the idle post-rejection socket into 13 re-logins in
  30 minutes against a locked account.
* The final status error keeps the full chain, so the server's reason is not
  replaced by "gave up reconnecting".
* Every step is written to `event_log` (`connection.connected`, `login_ok`,
  `login_rejected`, `lost`, `reconnect_scheduled`, `retry`, `gave_up`, `failed`,
  `closed`), per account, so a reconnect history can be read back instead of
  inferred from a 200-row packet window. See [`database.md`](database.md).
