# Architecture decisions

## Tauri over Electron

The brief mentioned both. Tauri is used because it satisfies the explicit Rust
framework requirement and uses the operating-system webview instead of bundling
another Chromium runtime. That directly supports the minimum-RAM objective. The
local API boundary means an Electron UI can still replace Tauri without changing
the network or persistence crates. In the distributable build, Tauri embeds the
daemon library in the same process; the standalone daemon binary exists only as
a development and network-testing entry point.

## Bounded data everywhere

| Resource | Bound |
| --- | ---: |
| SQLite protocol messages | 200 rows |
| Pending injection queue | 64 packets |
| SQLite connections | 1 |
| Accepted XT packet | 4 MiB |
| Injection lifetime | 1–60 seconds |
| `cra` packet floor | 4 seconds |
| Castle-switch floor | 3 seconds |

The daemon does not create per-packet files. Structured SQLite rows are both
developer-visible and suitable for migration in a packaged executable. Payloads
larger than the packet limit are represented by small metadata records rather
than copied into persistent storage.

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

## Runtime flow

```text
Tauri process
├── webview UI ──loopback API──┐
└── embedded Rust daemon <─────┘──direct client / relay──> game server
        │
        ├── response-driven login/navigation state machine
        ├── bounded injection queue
        └── SQLite (latest 200 messages + durable state)
```

The transparent browser proxy and direct-login client are adapters on the left
and right edges of this flow; neither is allowed to duplicate core state logic.

Task definitions are event descriptions, while task subscriptions are separate
data mapping task IDs to castle IDs. The timing gate is shared across task types:
it enforces the strict global `cra` floor, adds a castle-switch floor, and applies
smaller bounded variance to background traffic. Socket code does not decide
which castles subscribe to a recruit or attack event.
