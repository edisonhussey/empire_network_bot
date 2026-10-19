# Terrain cell architecture (prototype 1)

The terrain is a grid of independent cells. Each cell owns a small tilted surface defined by four corner heights. Neighbours may disagree, and a reconstruction pass fills the gaps between them with walls. This document is the rule set; the code lives in `core/` and must follow it.

Status: **prototype 1, validated**. A 100 × 100 world of 5 m cells generates in about 15–60 ms and meshes in about 15–35 ms. Seven node tests pass (`node core/tests/terrain.test.mjs`). Renders are in [`renders/`](renders/).

---

## 1. Data

```js
type CellSurface = [number, number, number, number];   // metres: [top-right, bottom-right, bottom-left, top-left]
interface TerrainCell { surface: CellSurface }
interface TerrainGrid { width: 100, depth: 100, cellSize: 5, heights: Float32Array /* width*depth*4 */ }
```

* **Corner order is fixed everywhere:** `TR = 0, BR = 1, BL = 2, TL = 3` (`core/terrain/grid.js`). Never reorder it, and never look a corner up by any other convention.
* **Storage:** one `Float32Array`, 4 floats (16 bytes) per cell, row by row (`index = cz * width + cx`). It holds no per-cell objects. `grid.surface(cx, cz)` returns a live 4-float view and `grid.cell(cx, cz)` returns a plain `{ surface }` copy (the `TerrainCell` shape).
* **Each cell is its own truth.** No corner is shared. Two cells that touch may give the same point two different heights. That disagreement is allowed and intentional: it is how steps and cliffs exist.
* Heights are generated at **centimetre precision** (e.g. `[1.72, 1.88, 1.87, 1.72]`).
* Optional per-cell side data lives in parallel arrays indexed the same way. The prototype has `zones: Uint8Array` (rolling, pad, terrace, plateau, slope, cliffband, irregular), which is for placement rules and debugging. Rendering does not need it.

## 2. Coordinates

* Three.js world, **Y up**, terrain on the **XZ plane**, **1 unit = 1 m**.
* +X is east ("right") and −Z is north ("top"). Seen from above with +X to the right, +Z points down the screen, the same direction rows run.
* Cell `(cx, cz)` covers `x ∈ [cx·5, cx·5 + 5]` and `z ∈ [cz·5, cz·5 + 5]`. The world spans 500 × 500 m from the origin.
* Corner positions: `TR = (x1, z0)`, `BR = (x1, z1)`, `BL = (x0, z1)`, `TL = (x0, z0)`. This is `CORNER_OFFSET` in `grid.js`.

## 3. Pass 1: top surfaces (triangulation)

Every cell becomes **two triangles split along the TL–BR diagonal, always the same way** (`core/terrain/surface.js`):

```
TL ------- TR        A = (TL, BL, BR)   used where local v >= u
 |  \   B  |         B = (TL, BR, TR)   used where local v <  u
 |   \     |         u = (x - x0) / 5 (west -> east), v = (z - z0) / 5 (north -> south)
 | A   \   |         both wound counter-clockwise from above, so normals point up
BL ------- BR
```

* The mesh builder (`mesh_top.js`) and every height query (`heightAt`, `surfaceNormalLocal`) use the **same** `TRIANGLES` table. As a result, an asset placed with `heightAt(grid, x, z)` sits exactly on the drawn ground. Never add a second interpolation (bilinear or otherwise).
* The output is non-indexed and flat-shaded: 6 vertices per cell, each triangle carrying its own face normal, so every cell reads as its own plane.

## 4. Pass 2: reconstruction (gap and cliff connectors)

`core/terrain/mesh_connectors.js`. Every cell inspects only its **east** and **south** edges, so each interior edge is handled exactly once.

| Edge | This cell | Neighbour | Plane | Wall faces |
|---|---|---|---|---|
| east | TR → BR | (cx+1, cz): TL → BL | x = x1 | ±X |
| south | BL → BR | (cx, cz+1): TL → TR | z = z1 | ±Z |

For the shared ground segment P → Q, side 1 is this cell's edge line and side 2 is the neighbour's. Let `dP = h1P − h2P` and `dQ = h1Q − h2Q`:

1. **Both |d| < 5 mm** (`EPS`): the edges meet. Nothing is added.
2. **Same sign, or one end joined**: one quad (two triangles), or a single triangle if one end is joined.
3. **Opposite signs**: the two edge lines cross at `t = dP / (dP − dQ)`. Two triangles are added, one on each side of the crossing point, facing opposite ways.

Rules:
* Walls are **vertical** (they lie in the edge's plane). Their normal is **horizontal and points toward the lower side**, and the triangle winding is flipped to match, so each wall is lit and seen from the side it faces.
* The **skirt** (`buildSkirt`) uses the same function against a flat base (the lowest height minus 4 m) along the map border, so the world reads as a solid slab.
* **Gap-free check (tested):** the total connector area equals ∫|h1 − h2| along every mismatched edge (the same-sign and crossing formulas in the test), to within 0.01%.
* Where four cells meet at different heights, the walls of the four edges all reach the shared vertical line, so there is no pinhole.

## 5. Chunks

`core/terrain/chunks.js`. The map is divided into chunks of 20 × 20 cells (100 × 100 m; a 100 × 100 map is 5 × 5 chunks). Each chunk is built independently from the global grid.

* A chunk **owns the connectors on its cells' east and south edges**, so walls along a chunk border are built once, by the west or north chunk. Reading the neighbour cell in the next chunk is fine because the grid is global.
* **Dirty rule:** editing cell `(cx, cz)` dirties its own chunk, plus the chunk west of it (if `cx` is the first column of its chunk) and the chunk north of it (if `cz` is the first row), because those chunks own the edges it shares with them (`chunksTouchedBy`).
* Builders return plain typed arrays and do not use three.js, so they can run in a Web Worker or in node unchanged.
* **Tested:** building chunk by chunk gives exactly the same triangle counts as building the whole map in one go.

## 6. World generation (`core/terrain/generate.js`)

**Rule: broad coherent landforms first; noise only perturbs them.** Never generate terrain as per-cell randomness.

1. Build one continuous function `landform(x, z)` in metres from named features: a rolling base (low-frequency fbm, about ±4 m over 240 m), a terraced hill (a dome cut into 2 m terraces), a mesa (a 12 m top with a cliff rim and one access ramp), an escarpment line (+2.5 m), a valley (4 m deep, 20 m flat floor, gentle sides), four **exactly flat building pads** (level = natural height rounded to 0.5 m), and a patch of shallow irregular cells.
2. For each cell, sample its centre (this gives the height and the zone), then sample its four corners. Each corner keeps `clamp(landform(corner) − centre, ±spread/2)`.
3. **Spread caps per zone:** rolling, plateau, cliffband and irregular get 0.2 m (the spec's usual limit); terrace gets 0.05; pad gets 0 (exactly flat); built slopes (the ramp and the valley sides) get 1.2 m, so they stay one continuous ramp instead of stairs.
4. A consequence of the cap: a cell can tilt at most 0.2 m over 5 m, which is **4%**. Ground steeper than that becomes small risers between cells, which pass 2 then fills. Cliffs are simply very large risers.
5. Generation is deterministic per seed (`mulberry32`, a stateless `hash2`, value-noise fbm in `core/math/`). The seed jitters feature positions; the layout stays the same.

Result for seed 7: 88.3% of cells keep spread ≤ 0.2 m (the rest are the built slopes, max 1.2 m), 426 cells are exactly flat, and the largest cliff is over 4 m. The mesh has 20,000 top triangles, 8,061 connector triangles on 4,438 edges, and 800 skirt triangles.

## 7. Rendering (`core/render/`)

* `stage.js` holds the renderer, camera, orbit controls, a soft grey hemisphere light plus a single sun at about 33° with PCF soft shadows, and ACES tone mapping. It knows nothing about terrain.
* `terrain_view.js` creates one `THREE.Group` per chunk containing the tops, connectors, skirt and cell outlines, and supports `rebuild(keys)`.
* Greyscale only: tops are shaded by height (one tone per triangle), connectors are darker, and the skirt darker still.
* Inspection aids:
  * **Cell outlines**: each cell's own quad edge, which shows independence and fades out with distance.
  * **Highlight connectors**: tints the walls amber.
  * **Wireframe**.
  * **Relief ×1/×2/×4**: a vertical scale on the view only. Data and picking are unchanged.
  * **Hover**: outlines the cell under the mouse and prints its four corners and zone.
* Picking uses the face index. Triangles are emitted 2 per cell, row by row, so `cell = range.x0 + (faceIndex >> 1) % w`. There is no geometric guessing at borders.

## 8. Layers and dependencies (hard rule)

```
core/math        rng, noise                          (no deps)
core/terrain     grid -> surface -> mesh_top / mesh_connectors -> chunks; generate   (math only; NO three.js)
core/render      stage, terrain_view                 (three.js + core/terrain)
core/app         page bootstrap and UI               (everything above)
```

`core/` imports nothing from the old `src/` prototype. New systems (assets, roads, navigation) go in new folders under `core/` and depend on `core/terrain` through its query API (`heightAt`, `surfaceNormalLocal`, `spread`, `zones`), never on its meshes.

## 9. Validation checklist (before changing these rules)

- [ ] `node core/tests/terrain.test.mjs` passes, including the gap-free area test and the chunk-equals-whole test.
- [ ] Corner order is still TR, BR, BL, TL in the data, the builders, the tests and this document.
- [ ] `heightAt` equals the drawn surface (centroid test).
- [ ] Connector normals are horizontal and face the lower side (look at `05_connectors_highlighted` from both sides).
- [ ] The Wide, Cliffs, Terraces and Escarpment views show no holes at ×1 and ×4 relief.
- [ ] Generation and meshing for 100 × 100 cells stay under about 100 ms each.

## 10. Known limitations of prototype 1

* **Fixed diagonal.** Non-planar ("saddle") cells always fold along TL–BR. That is correct but direction-biased. Cells within 0.2 m make this almost invisible; built slopes show it.
* **Slope capacity is 4%** under the 0.2 m rule. Gentle rolling ground therefore becomes fine stairs of 0.1–0.25 m risers (visible as speckle from far away). This follows directly from the spec, but needs a decision (see the next section).
* **T-junctions.** A connector's crossing point, and its corners, lie on top-surface edges that have no vertex there. This can show hairline cracks at extreme zoom on some GPUs, though none were seen in the renders.
* **Vertical walls only.** There are no sloped cliff faces, overhangs or rounded rims. Long straight cliffs are made of many 2-triangle wall pieces, unmerged.
* **No UVs or material IDs** yet, and shading is flat per triangle.
* **Not wired** into the old map app (castles, routing, LOD) and does not use its 10 m node grid. This is a fresh base.
* Headless renders came out at 756 px wide even when 1600 was requested (a Chrome quirk on this machine). The live page renders at full resolution.

## 11. Next iteration

1. **A per-cell diagonal bit** (pick the split that makes the flatter pair of triangles) stored beside the heights. `surface.js` stays the single source of truth.
2. **An edge classification** from data alone: for each east and south edge, store `step = max |d|`, then label it joined (< 5 mm), riser (< 0.5 m, walkable), or cliff (blocks walking). This gives navigation and road placement without ever touching meshes.
3. **A placement API** under `core/placement/`: `footprintFits(rect, maxSpread, maxStep)`, `flattenFootprint(rect)` (rewrite cells, then rebuild the dirty chunks), and `normalAt` for tilting props.
4. **Merging coplanar connector runs** along straight cliffs, and optionally sloped rims for cliffs of 1–3 m.
5. **Materials:** a per-cell material byte plus world-space or triplanar texturing for tops and walls.
6. **Workers and streaming:** run `buildChunk` in a worker with transferable arrays; stream chunks for large maps (16 bytes per cell, so a 1000 × 1000 map is 16 MB of heights).
7. **Far LOD:** merge runs of flat or coplanar cells into larger quads for distant chunks.
8. Decide whether gentle ground should keep the 0.2 m cap (fine stairs) or allow larger spreads in "rolling" zones as well.

## Renders

| File | Shows |
|---|---|
| `01_wide.png` | the whole 500 m world: terraced hill, mesa with ramp, escarpment, valley, pads |
| `02_wide_relief_x2.png` | the same at ×2 relief (inspection only) |
| `03_cells_hover.png` | the pad edge in the east lowland: cell outlines, a hovered cell and its four corners in the panel |
| `04_mesa_cliffs_ramp.png` | the mesa's cliff rim made of reconstructed walls, and the continuous sloped ramp |
| `05_connectors_highlighted.png` | the same with all connector walls tinted |
| `06_terraces.png` | the stepped hill: flat terraces with 2 m risers |
| `07_escarpment.png` | a long low cliff line with half-height transition cells |
| `08_wireframe.png` | the triangles: 2 per cell plus the wall pieces |

Run it: `node serve.mjs 5174` (from `interactive_world/`), then open http://127.0.0.1:5174/core/ (options: `?seed=7&view=wide|cells|cliffs|terraces|scarp&relief=2&highlight=1&wire=1&hover=cx,cz&ui=0`).
