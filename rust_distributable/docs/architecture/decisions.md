# Design decisions

Why the stack is shaped the way it is. See [`README.md`](README.md) for the other parts.

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
