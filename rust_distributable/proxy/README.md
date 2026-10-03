# Transparent proxy adapter

This adapter will terminate only the target Goodgame CONNECT/TLS connection,
relay its WebSocket through `empire-daemon`, and tunnel all unrelated hosts
without inspection. It requires a generated local CA, explicit certificate
installation, hostname allow-listing, and certificate/key storage in the OS
keychain. The relay/core APIs are already independent of this adapter.

