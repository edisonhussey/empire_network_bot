# Tile terrain architecture (1 m tiles, 0.2 m steps)

The fine terrain layer after the "Quantized Heightmap Terrain" guide and the "Quantized 1×1 Terrain System" sheet: a 1 m logical grid whose corner heights are whole 0.2 m levels, rendered as a smooth, dressed world. It follows the same rules as the 5 m cell terrain ([TERRAIN_CELL_ARCHITECTURE.md](TERRAIN_CELL_ARCHITECTURE.md)): every tile owns its four corners, neighbours may disagree, and walls fill the gaps. A 5 m cell is 5 × 5 tiles.

Status: **prototype, validated** on a 48 × 40 example island (`core/tiles.html`). Seven node tests pass (`node core/tests/tiles.test.mjs`); the kit passes `node assets/development/scripts/kit_check.mjs`. Renders: `renders/tiles_*.png`.

---

## 1. Data (`core/tiles/tile_grid.js`)

```js
TileGrid { width, depth,
  levels: Int16Array(width * depth * 4),   // corner levels [TR, BR, BL, TL], 1 level = STEP = 0.2 m
  ground: Uint8Array(width * depth),       // GROUND: grass, path, rock, sand, bed (stream bed), pad
  water:  Int16Array(width * depth) }      // water surface level, or NO_WATER
```

* Corner order and frame are the 5 m grid's: `TR, BR, BL, TL`, +X east, −Z north, tile `(tx, tz)` covers `x ∈ [tx, tx+1]`, `z ∈ [tz, tz+1]`.
* Heights are integers. Metres = level × 0.2. Nothing is stored in metres.
* **Inside a tile the corners differ by at most one level.** Between tiles anything goes: that is how risers and cliffs exist. `grid.firstInvalid()` finds a tile that breaks the rule.

## 2. The five tile shapes (`core/tiles/tile_shapes.js`)

Subtract the lowest corner and each corner is 0 or 1. Up to rotation there are exactly five patterns:

| shape | canonical `[TR, BR, BL, TL]` | what it is | rotations |
|---|---|---|---|
| `flat` | `[0,0,0,0]` | level ground | 1 |
| `corner_high` | `[1,0,0,0]` | one corner raised (NE) | 4 |
| `corner_low` | `[0,1,1,1]` | one corner lowered (NE) | 4 |
| `ramp` | `[1,0,0,1]` | rises one step to the north | 4 |
| `saddle` | `[1,0,1,0]` | two opposite corners raised | 2 |

These are the sheet's five shapes (the sheet writes its corners in its own order, e.g. one low corner as `[1,1,1,0]`; up to rotation they are the same). Blender render of all five with their rotations: `assets/development/previews/terrain_kit_tile_shapes.png`.

* `classify(corners)` → `{ shape, rotation, base }`. Rotation `r` = quarter turns counter-clockwise seen from above = three.js `rotation.y = r·π/2`, the same table as `FACING_ROTATION` (`r` 0 N, 1 W, 2 S, 3 E). One quarter turn moves corner `k` to `(k + 3) % 4`.
* **Surface rule:** the top is the **bilinear** patch through the four corners, sampled on 4 × 4 sub-quads (32 triangles per tile). `tileHeightAt(grid, x, z)` and `tileNormalAt` use the same bilinear formula; the drawn triangles deviate from it by at most 3 mm (tested). Props stand on `tileHeightAt`.
* The tile meshes are authored in Blender (`Tile_*` in `terrain_kit.glb`) from the same canonical table; `kit_check.mjs` fails if any tile vertex is off the bilinear patch.

**Inside a region every edge joins** (the sheet's rule: at most one step across any edge). The example scene builds one vertex field of whole levels per region (rolling ground plus named small forms: a ridge, diamond mounds that meet diagonally in saddle tiles, a swale), limited so every vertex is within one level of its eight neighbours; all tiles of the region share those corners. Risers and cliffs only appear where regions meet (tested).

## 3. Edges (`core/tiles/tile_edges.js`)

`classifyEdges(grid)` reports every tile edge whose sides disagree (each tile's east and south edge, so every interior edge once; optionally the map border against the skirt base), with the side it faces (towards the low side), foot and top heights and its kind:

| kind | rule | drawn as |
|---|---|---|
| `riser` | largest step ≤ 0.4 m (2 levels) | earth bank (`Bank_*`) + grass lip; walkable |
| `cliff` | taller | rock faces (`Cliff_Low*` up to 1.25 m, else `Cliff_A/B/C`), corner posts, grass lip, rubble; blocks walking |
| `border` | map edge | rocky rim (`Cliff_*`) over the soil skirt |

A cliff edge between two water tiles is a **waterfall** (`dressGrid().falls`): it gets a water sheet instead of rock.

## 4. Meshes (`core/tiles/tile_mesh.js`, plain typed arrays, no three.js)

* `buildTileTops(grid, templates, range)`: each tile is its shape's template, turned and lifted to its base level; the copies are merged per chunk (16 × 16 tiles) and given **smooth normals** by welding coincident vertices. This is the guide's "same data, smooth result": the logical surface is exactly the quantized corners, only the shading blends. Where neighbours disagree the vertices do not coincide, so cliff edges stay crisp. `tileOfTri` maps each triangle back to its tile (picking).
* `buildTileWalls(grid, range)`: `fillEdgeGap` from the 5 m terrain on tile heights, so the gap-free guarantee carries over (tested: wall area = ∫|h1 − h2| on every edge). Each vertex carries `soil` (risers and skirt 1, rock cliffs 0). The kit pieces dress the walls; the walls guarantee no holes behind them.
* `buildWater(grid, range)`: one quad per water tile at its water level, plus side faces where water meets the map border.

## 5. Dressing (`core/tiles/dress.js`)

`dressGrid(grid, { seed, region, ramps, distToPath, exclude })` → `{ pieces: { [name]: [{ x, y, z, ry, s, sx, sy, tilt }] }, edges, falls }`. Pure data, deterministic per seed.

* Edge pieces follow the edge classes (section 3). They are authored facing north and turned with `ry = atan2(−nx, −nz)` to look along the edge's face direction; cliff faces and banks are one unit tall and get `sy` = the step they cover.
* Scatter pieces follow broad noise fields plus per-tile hashes: forest clumps (conifers on the uplands, broadleaf / round / blossom trees in the low ground), bushes along cliff tops and the path edges, boulders, grass tufts and flower patches on every green tile, reeds on sand beside water, pebbles in the stream, log steps across every ramp. Trees keep spacing and stay off the path, the water's edge and the area in front of buildings (`exclude`).
* **Scenery does not snap to the height grid:** pieces sit at `tileHeightAt` with free position, yaw and scale, and rocks lean onto `tileNormalAt`.

## 6. The terrain kit (`assets/development/terrain_kit.glb`)

Built by `assets/development/scripts/terrain_kit_textures.py` → `terrain_kit_build.py` → `terrain_kit_export_glb.py` in `terrain_kit.blend` (live Blender). Frame and export follow [ASSET_GLB_PROTOCOL.md](ASSET_GLB_PROTOCOL.md): Blender +Y = game north (−Z), every piece one node at the origin on the ground, custom properties exported as node extras (`kind`, `shape`, `scale_y`, ...).

| group | pieces | tris | technique (optimization.md) |
|---|---|---|---|
| tiles | `Tile_flat`, `Tile_corner_high`, `Tile_corner_low`, `Tile_ramp`, `Tile_saddle` | 32 each | exact bilinear patches |
| cliffs | `Cliff_A/B/C` (rock columns), `Cliff_Low`, `Cliff_Low_B` (boulder steps), `Cliff_Corner` | 95–240 | T08 faceted silhouettes, T11 AO / moss vertex colour |
| edges | `Bank_A/B` (earth riser), `Lip_Grass` (hanging fringe) | 64 / 8 | T13, T01 |
| path | `Path_Step` (log step), `Path_Stones` (stepping stones) | 44 / 60 | T08 |
| rocks | `Rock_Boulder`, `Rock_Medium`, `Rock_Pebbles`, `Rock_Slab` | 80–400 | T08, T11 |
| trees | `Tree_Broadleaf`, `Tree_Round`, `Tree_Blossom`, `Tree_Conifer` | 230–820 | T03 leaf masses + T01 leaf cards, T11 crown light |
| bushes | `Bush_Round`, `Bush_Dark`, `Bush_Blossom`, `Bush_Fern` | 10–260 | T03 + T01 |
| ground cover | `Grass_Tuft`, `Grass_Tall`, `Flowers`, `Reeds` | 6–8 | T15 crossed cards |

Materials: 8 (terrain, rock, soil, bark, three tiling leaf textures, one foliage alpha atlas). Ground textures for the game shader (`terrain_grass/dirt/sand.png`) are painted by the same script and not embedded in the GLB.

## 7. Rendering (`core/render/tile_view.js`)

* Ground shader: sloped tiles show worn earth through the grass (by the surface normal, as on the sheet's tiles); grass / dirt / sand textures in world space (4 m repeat, a second scale against tiling), blended by a per-tile splat texture (path, sand, bed) with linear filtering and noise so path edges are soft; a 2-texel-per-metre occlusion texture darkens cliff feet and the ground under crowns; optional 1 m grid lines.
* Walls: world-projected rock or soil by the `soil` attribute, darker towards the bottom of the island.
* Water: transparent, animated ripple normals, shallow/deep colour and a foam line from the splat's water channel. Waterfalls: bent animated sheets with free-end fading and foam on the pool.
* Pieces: one `InstancedMesh` per piece primitive (about 35 meshes for ~4,500 instances); foliage cards are alpha-tested, double-sided, lit with their authored normals, and sway in the wind.
* Lighting on the page: warm sun from the south-west with 4096 soft shadows, blue hemisphere fill, sky gradient, ACES.

Measured on the example on this Mac (`core/tiles.html`, slope-path view, renderer.info): 61k top + 1k wall triangles, ~158k instanced piece triangles, **65 draw calls**, scene build ~90 ms.

## 8. Placing buildings

A building footprint on tiles is `placeOnCells({ cx, cz, w, d, facing, cellSize: 1 })` on a rect of flat tiles at one level (`levels` equal). The shrine (15 × 10 m) sits on the plateau pad turned **W**, covering tiles x 34–43, z 2–16, its stair meeting the path.

## 9. Known limitations

* Water and cliffs follow the 1 m grid in plan, so diagonal shores and cliff lines step in 1 m stairs (as in the guide's top-down grid). Rounded shore meshes would hide it.
* Tall cliffs stretch one rock module vertically (up to ~3 m reads fine; taller cliffs should stack modules).
* No LOD or chunk culling for pieces yet; every instanced mesh is one batch for the whole map.
* Tiles are not wired into the 5 m world yet; the example scene is its own page.

## 10. Run it

`node serve.mjs 5174`, open http://127.0.0.1:5174/core/tiles.html (`?view=hero|shrine|falls|ramp|rise|forms|top|wide&grid=1&props=0&wire=1&hover=tx,tz&ui=0`). Screenshots: `node --experimental-websocket tools/shot.mjs out.png "<url>"`.
