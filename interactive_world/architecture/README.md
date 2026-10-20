# interactive_world architecture

This folder holds the rules for the **new base** in `interactive_world/core/`. Read these documents before building on `core/`, and update them when a rule changes.

| Document | What it fixes |
|---|---|
| [TERRAIN_CELL_ARCHITECTURE.md](TERRAIN_CELL_ARCHITECTURE.md) | the terrain: per-cell corner heights, triangulation, reconstruction walls, chunks, generation, validation |
| [TILE_TERRAIN_ARCHITECTURE.md](TILE_TERRAIN_ARCHITECTURE.md) | the 1 m tile terrain: 0.2 m levels, the five tile shapes, edge classes, dressing with the terrain kit, the example scene |
| [ASSET_GLB_PROTOCOL.md](ASSET_GLB_PROTOCOL.md) | building GLBs: frame (front = north = −Z), origin, footprint in cells, Blender export, placement and facing |
| [renders/](renders/) | reference renders of terrain prototype 1 and the tile scene (`tiles_*.png`) |

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
  tiles.html            the tile terrain scene (http://127.0.0.1:5174/core/tiles.html)
  math/                 rng.js (mulberry32, hash2), noise.js (value noise, fbm, smoothstep)
  terrain/              grid.js, surface.js, mesh_top.js, mesh_connectors.js, chunks.js, generate.js
  tiles/                tile_grid.js, tile_shapes.js, tile_edges.js, tile_mesh.js, dress.js, scene_example.js (1 m tiles, no three.js)
  placement/            footprint.js (placeOnCells, facing N/E/S/W, footprintLevel)
  render/               stage.js, terrain_view.js, tile_view.js (tiles + kit pieces), asset_view.js (building GLBs), compass.js (N/E/S/W letters + rose)
  app/                  terrain_demo.js (5 m cells + shrine), tile_demo.js (tile scene + shrine on the plateau)
  tests/                terrain.test.mjs, placement.test.mjs, tiles.test.mjs   (node core/tests/<file>)
```

The static server is `interactive_world/serve.mjs` (`node serve.mjs 5174`).
