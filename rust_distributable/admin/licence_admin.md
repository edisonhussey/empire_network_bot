# Offline licence administration

The admin binary is an issuer tool and must never be included with customer
downloads. The application contains only the public key in
`crates/empire-core/assets/licence_public_key.b64`.

## Key model

One Ed25519 private key signs every licence issued under key ID
`oa-main-2026`. Ed25519 deterministically gives that private key one matching
public key. Every future OpenAuto build embeds a copy of that same public key;
individual customers do not need separate public keys.

The verifier is implemented as a keyring so a future release can trust both an
old and replacement public key during rotation. Rotation requires generating a
new independent keypair and shipping its public key in an application update.
Do not try to derive many Ed25519 public keys from the master private key.

The private key currently lives at `admin/master_key.txt`, has Unix mode `0600`,
and is excluded by `rust_distributable/.gitignore`. Back it up offline in an
encrypted password manager or hardware-encrypted volume. Losing it prevents
renewals; disclosing it lets an attacker mint licences.

## Generate the issuer key once

```sh
cargo run -p empire-license-admin -- keygen \
  --private admin/master_key.txt \
  --public crates/empire-core/assets/licence_public_key.b64
```

The command refuses to overwrite an existing private key. Do not run it again
for ordinary customers or renewals.

## Issue the four-day test token

```sh
cargo run -q -p empire-license-admin -- issue \
  --private admin/master_key.txt \
  --license-id customer-alice \
  --subject alice@example.com \
  --server US1 \
  --bootstrap-x 509 \
  --bootstrap-y 405 \
  --days 4 \
  --revision 1 \
  --output admin/alice-4-day.token
```

The output file contains one `OA2.<payload>.<signature>` stage-0 token and is created
with mode `0600` on Unix. Omit `--output` to print it instead. The default feature set is
`game_network,account_initialize,automation` and the default tier is `pro`.
Use `--features` or `--tier` to override them.

## Renew or change a plan

Issue another token with the same `license-id`, a strictly higher revision,
and the desired later expiry:

```sh
cargo run -q -p empire-license-admin -- issue \
  --private admin/master_key.txt \
  --license-id customer-alice \
  --subject alice@example.com \
  --server US1 \
  --bootstrap-x 509 \
  --bootstrap-y 405 \
  --days 365 \
  --revision 2 \
  --tier pro
```

The user selects **Add credits / update plan** and pastes the replacement
token. The app rejects an older revision, a different token reusing the same
revision, and a renewal that shortens the installed licence.

## Security properties and limits

- Payload bytes are signature-verified before JSON is parsed or trusted.
- Editing `expires_at`, features, tier, subject, or revision invalidates the
  signature.
- Game WebSockets, relays, packet injection, account discovery, and automation
  require a currently valid feature entitlement.
- Active sockets recheck the licence every five seconds and close after expiry.
- SQLite remembers the highest observed wall-clock time and rejects a rollback
  greater than five minutes.
- Fully offline software can still be binary-patched by a determined attacker,
  and a user with filesystem control can replace local state. A future online
  activation service and trusted timestamp can raise that barrier further.

## One licence per game account

Each game account needs its own licence, with its own `--license-id`. Several can be
installed side by side; adding a token with a new id never replaces the others.

1. Have the account log in once. Without a licence it is refused, and the refusal
   records exactly what to issue for: the window shows
   `server WORLD2, player N, main castle (X, Y)`, and the same line is in the
   database (`event_log`, kind `licence.needed`).
2. Issue a stage-0 token for those values:

   ```sh
   cargo run -q -p empire-license-admin -- issue \
     --private admin/master_key.txt \
     --license-id pingpoko-stage0 \
     --subject Pingpoko \
     --server WORLD2 \
     --bootstrap-x X \
     --bootstrap-y Y \
     --days 30 \
     --output admin/pingpoko-stage0.token
   ```
3. Paste it in **Add credits / update plan**. It stays "unactivated" until that account
   logs in; the first login with a matching server and main castle binds it to the
   player id for good.

The server name must be `US1` or `WORLD2` (the endpoints the app trusts). The main
castle is the one in the base kingdom (kingdom 0, the player's first castle), not the
Sands castle.

Renewing is the same command with the account's existing `--license-id`, a higher
`--revision`, and a later expiry. Each licence expires on its own.

## Issued tokens (8 Oct)

Both are 30-day, pro tier, default features, signed with `admin/master_key.txt`.

| File | Licence id | Server | Main castle | Revision | Use |
| --- | --- | --- | --- | ---: | --- |
| `admin/ventrilo-stage0-30-day.token` | `ventrilo-stage0` | US1 | 509, 405 | 2 | Renewal of the installed licence (it expired 8 Oct 09:27). Paste into **Add credits / update plan**. |
| `admin/pingpoko-stage0-30-day.token` | `pingpoko-stage0` | WORLD2 | 552, 522 | 1 | New licence for Pingpoko. Binds the first time that account logs in. |

Pingpoko's main castle is "poole" at 552, 522 in kingdom 0 (confirmed by the owner;
also seen in the 16 Sep capture, player 411818). Its Sands castle at 722, 533 is not
the main castle. If the account's main castle has moved since, the first login is
refused with the real coordinates in the message and in `event_log`
(`licence.needed`); reissue with those.

`admin/*.token` is git-ignored.

Install order matters: use a build whose `/v1/health` reports `api_version` 44 or
later before adding a second account's token. Older builds keep a single licence
and a new token would replace the installed one.
