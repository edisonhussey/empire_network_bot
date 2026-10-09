# Runtime flow

How the window, the embedded daemon and the game server fit together.

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
