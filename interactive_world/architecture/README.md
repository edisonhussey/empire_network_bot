# interactive_world architecture

This folder holds the rules for the **new base** in `interactive_world/core/`. Read these documents before building on `core/`, and update them when a rule changes.

| Document | What it fixes |
|---|---|
| [TERRAIN_CELL_ARCHITECTURE.md](TERRAIN_CELL_ARCHITECTURE.md) | the terrain: per-cell corner heights, triangulation, reconstruction walls, chunks, generation, validation |
| [renders/](renders/) | reference renders of terrain prototype 1 |

## Ground rules for `core/`

1. **Units and frame.** 1 unit = 1 m, Y up, ground on XZ, +X east, −Z north. Every asset and every system uses this frame.
2. **Data first.** Every system is plain data (typed arrays) plus pure functions that run in node. three.js appears only in `core/render` and `core/app`.
3. **One source of truth per question.** Ground height comes only from `terrain/surface.js`, and the triangulation table is shared by meshing and queries. Never re-derive either.
4. **Chunk-local work.** Builders take a cell range and return arrays. Edits rebuild only the chunks they touch.
5. **Broad structure before noise.** Generated content starts from named, intentional forms; noise only perturbs them.
6. **Tested invariants.** Each system has a node test file in `core/tests/`, and rules in these documents are backed by a test where possible.
7. **No legacy imports.** `core/` does not import from `src/` (the previous map prototype). Reuse ideas from it, not code.

## Layout

```
core/
  index.html            the terrain prototype page (http://127.0.0.1:5174/core/)
  math/                 rng.js (mulberry32, hash2), noise.js (value noise, fbm, smoothstep)
  terrain/              grid.js, surface.js, mesh_top.js, mesh_connectors.js, chunks.js, generate.js
  render/               stage.js, terrain_view.js
  app/                  terrain_demo.js (bootstrap, panel, views, hover)
  tests/                terrain.test.mjs   (node core/tests/terrain.test.mjs)
```

The static server is `interactive_world/serve.mjs` (`node serve.mjs 5174`).
