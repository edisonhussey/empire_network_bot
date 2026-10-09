# Security boundary and licence trust

What the loopback API trusts, and how licences are signed and renewed.

## Security boundary

The daemon binds to loopback and permits only the known development and packaged
Tauri origins. A later hardening pass can replace HTTP injection with Tauri IPC
or a per-launch bearer capability. Licence verification uses Ed25519: the
private key remains in an external issuer, while the application contains only
the public verification key. Claims contain `not_before` and `expires_at`
epochs.

## Licence trust chain

```text
offline master private key
        │ signs exact payload bytes
        ▼
OA1.<base64url payload>.<base64url Ed25519 signature>
        │
        ▼
embedded public-key ring → verify signature → parse JSON → check time/revision/features
        │
        ├── invalid: activation UI only; no game network capability
        └── valid: explicitly entitled features may run
```

There is one primary issuer key, not one keypair per installation. All builds
carry the same public key. The keyring abstraction exists solely for controlled
future rotation. Renewal is a higher signed revision of a licence, which avoids
replayable unsigned “credit” counters.

## One licence per game account

A licence is issued for one game account: the signed claims carry the server and the
main-castle coordinates, and the first login with a matching account binds the
licence to that player id permanently (`licence_activation`, unique per server and
player). Any number of licences can be installed (`licence` table, schema V20); a
login uses the one bound to its account, or binds an unbound one whose signed
coordinates match, or is refused with the server, player id and castle a licence
would have to be issued for. Each live session re-checks its own licence every few
seconds, so one account's expiry never stops another.

See [`../problems/2.md`](../problems/2.md) for the failure that led to this and
[`../../admin/licence_admin.md`](../../admin/licence_admin.md) for issuing licences.
