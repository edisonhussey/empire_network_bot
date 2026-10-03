# Task 2 — licensed desktop foundation

## Delivered

- One offline Ed25519 issuer key and one public key embedded in every build.
- Compact signed `OA1` tokens with schema, issuer key ID, licence ID, subject,
  issue/start/expiry timestamps, revision, tier, and feature entitlements.
- Signature-before-deserialization verification, expiry enforcement, feature
  gates, renewal anti-rollback, and local clock-rollback detection.
- A local activation and renewal API. No game connection is possible before a
  valid token is installed, and active transports stop when it expires.
- An Apple-style first-launch activation screen and an in-app plan update path.
- SQLite in the operating system application-data directory when packaged by
  Tauri. It stores licence state, bounded network history, account metadata,
  castles, commander IDs, and observed RBC targets.
- Account initialization using the native Rust connection. Passwords remain in
  memory only; they are not placed in SQLite or packet history.

## Four-day acceptance test

The real issuer generated a four-day token. A clean daemon rejected protected
endpoints with HTTP 403 before activation. After activation it reported the
expected subject, `pro` tier, revision 1, and all three features. The token
remained active after a daemon restart. A revision 2 renewal extended it to
eight days, and a subsequent revision 1 token was rejected.

## Local data layout

Tauri resolves the directory using `app.path().app_data_dir()`, so the packaged
application does not write beside its executable. The database name is
`empire.sqlite3`. Platform directory resolution is owned by Tauri/OS rather
than hardcoded home-directory paths.

Passwords are deliberately excluded from SQLite. Production persistence should
use an OS credential store or a Stronghold vault with an explicit user unlock;
plaintext credential storage was not accepted merely to make initialization
appear automatic.
