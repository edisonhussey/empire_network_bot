# Clear the build cache

```zsh
oa clean
```

That is shorthand for these two commands (run from `rust_distributable/`):

```zsh
cargo clean --profile dev
cargo clean --profile dev --manifest-path desktop/src-tauri/Cargo.toml
```

It deletes only the **debug** build output: `target/debug` (test and check builds, usually the biggest, around 9 GB) and
`desktop/src-tauri/target/debug` (around 1 GB). Source code, `target/release`, `releases/`, the database and the game data
are not touched, and the next `cargo test` or `cargo build` rebuilds what it needs (a few minutes).

What it is and what is safe: [`../docs/cleanup/clear_cache.md`](../docs/cleanup/clear_cache.md).
