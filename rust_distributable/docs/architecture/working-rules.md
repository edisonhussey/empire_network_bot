# Working rules

How changes are made in this repository.

1. **Contract first.** Agree the types and the API shape before any implementation.
2. **Move, then change.** Refactors move code verbatim; behaviour changes are separate commits.
3. **Verify every step.** `cargo check --workspace --tests`, `cargo test --workspace`, and `npx vite build` in `desktop/` must pass before moving on.
4. **Bump `API_VERSION`** whenever behaviour the window depends on changes.
5. **New responsibility, new module.** If an edit adds a concern to a file, it belongs in its own file.
