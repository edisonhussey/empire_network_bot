# interactive_world

> **New base (2026-10-19):** fresh work goes in `core/`, ruled by `architecture/` (start with `architecture/README.md`). The terrain is now per-cell corner heights with reconstruction walls (`core/`, http://127.0.0.1:5174/core/). The 1 m tile terrain with the terrain kit and the shrine is at http://127.0.0.1:5174/core/tiles.html (`architecture/TILE_TERRAIN_ARCHITECTURE.md`). Everything below describes the previous map prototype in `src/`, kept for reference; `core/` does not import it.


Prototype of the 3D, cartoon-stylised game view. Zero dependencies (three.js r160 is vendored in `vendor/`). Read `CONTEXT.md` first when resuming:
it holds the goal, the decisions already made and what is intentionally unbuilt.

    node serve.mjs 5174              # http://127.0.0.1:5174/            model viewer (castle, tower, every map asset; lighting; team colours)
                                     # http://127.0.0.1:5174/world.html  the map: kingdoms, land, castles, attacks
    node tools/build_castle.mjs      # assets/castle.glb       the keep (pivot = footprint centre, see extras.pivot)
    node tools/build_tower.mjs [n]   # assets/tower.glb        robber baron watchtower, n shaft segments (2 = the sheet's 4.5 units)
    node tools/build_tree.mjs [--height 18] [--keep-apex] # assets/pine.glb  the pine + its own pebbles / grass / sparkles, stripped from the downloaded assets/tree.glb, in metres
    node tools/build_map_library.mjs # assets/map_library.glb  26 map pieces + assets/preview/<group>.glb for the viewer
    node tools/build_army.mjs        # assets/army.glb         13 soldiers, clips Idle/March/Attack/Cheer
    node tools/test_world.mjs        # 28 tests: pathfinding, land, bridges, generator, marches, scale

## Layers (each can change without the others)

| Layer | Files | Knows about |
|---|---|---|
| World rules (pure data, runs in node) | `src/world/`: `world.js` (kingdoms, castles, NPCs, marches), `terrain.js` (box features), `spatial.js`, `rect.js`, `focus.js`, `scenario.js`, `config.js`, `npc_kinds.js` | nothing visual |
| The land (pure) | `src/world/mapfield/`: `field.js` (MapField), `generate.js`, `sheet_example.js`, `noise.js`, `curves.js`, `solids.js`, `constants.js` | nothing visual |
| Route drawing (visual only) | `src/world/`: `routing.js` (Router), `fieldgrid.js`, `pathfinding.js` (A*), `curve.js`, `polyline.js`, `grid.js` | the world and the land |
| Teams / colours | `src/world/teams.js`, `team_slots.js` | materials only |
| Map app (mountable) | `src/map/app.js` composes `stage` `ground_view` `water_view` `props_view` `library` `tree_asset` `castle_view` `npc_view` `march_view` `arrow` `grid_view` `terrain_view` `ui` `labels`; `map.js` + `world.html` bootstrap it | the world, the land, the library |
| Shared rendering | `src/render/outline.js` (cartoon outline, shadows, hulls) | three.js |
| Model viewer | `index.html`, `src/viewer.js`, `src/army.js` | the GLB files |
| Asset generators | `tools/build_*.mjs`; castle `tools/parts/*`; squad `tools/army/*`; tower `tools/tower/*`; map pieces `tools/mapkit/*` (`catalog.mjs` lists them); shared `tools/lib/*` | produce `assets/*.glb` |

## World model

* Coordinates are integer grid **nodes** (grid-line intersections): x right, y down the map (= scene z); scene position = node x `CELL` = 10 (**1 scene unit = 1 m**, so a node is 10 m).
  Castles and NPCs sit on nodes and take a 3x3-node footprint. Their models are centred on the footprint (the castle's `extras.pivot`), so a squad's
  route starts and ends on the building's footprint edge.
* A march is `from -> to` over `duration` seconds from `startedAt`; it arrives at `startedAt + duration` whatever the drawn route looks like.
  `world.progress(m)` is the fraction done; the renderer places the squad at `progress x route length` along the drawn route, at the land's height.
* Only the camera's window is rendered: `world.queryWindow(rect)`, `world.marchesInWindow(rect)` and `field.propsIn(rect)` feed the views.

## The land (MapField)

A spec (rivers, roads, forests, rocks, outcrops, fields, buildings, props; same format whether hand-written or generated) is turned into answers about
any point: `heightAt`, `walkHeightAt` (bridge decks), `isWater`, `isBridge`, `surfaceAt` (road / bank / soil / rock weights), `costAt` (roads are quicker),
`blockedAt(x, y, clearance)`, `propsIn(rect)`, `rasterise(rect)` (the bitmap of boundaries: bit 1 water, bit 2 solid, plus heights). Nothing is stored per
node, trees and ground cover are scattered per 8-node chunk on demand, so a 1286-node map costs only what the window shows.
* Bridges are created automatically where a road crosses a river, spanning straight across it.
* `generateMap({ size, seed, castles, npcs, rivers, forestCover, farms, houses, towers, rocks, outcrops })` writes a spec; `sheet_example.js` is a hand-authored
  one laid out after the reference sheet. `scenario.js` has `SHEET` and `generated(seed)`; `world.setField(spec)` swaps the land under existing castles.
* Pathfinding reads the land lazily (`FieldGrid`). Squads prefer roads (cost 0.6), keep clearance from rocks/buildings/fields and a smaller margin from trees, and
  cross water only on bridges. If a castle is walled in by forest the router relaxes its clearances in steps (`ROUTE.levels`) before drawing a straight line.

## Asset library (map_library.glb)

Every piece is built individually by a parametric builder in `tools/mapkit/` (position, rotation and scale are applied at placement; heights come from
builder options or the spec's `h` / `height`): grass tufts (no trees: the pine is its own asset, below), stone nodes (s/m/l), rocky bump, cliff edge (two heights), slope piece,
bank stones, crop field, hay bales, fence, thatched and tiled houses, farm building, small watchtower, bridges of 3..8 tiles, a dock. `catalog.mjs` is the list;
add a builder and an entry, rebuild, and the field can place it by name. The tower (`tools/tower/tower.mjs`) takes `segments` for height.

## Scale: 1 scene unit = 1 metre

`CELL = 10` (config.js): a node is 10 m, the 3x3-node castle island 30 m. Landmarks are scaled to stand `BUILDING_HEIGHT` = 24 m (castle, robber baron tower; measured from the model at load, `app.js`).
The older library pieces were modelled at 1.25 units per tile, so `src/map/asset_scale.js` places each at `LEGACY_SCALE` (= CELL / 1.25 = 8, which keeps rocks, cliffs and fields filling the same
nodes) unless it has a real-world size: houses 8.4 m wide, barn 18 m, watchtower 14 m, fences 1.2 m high and 10 m per segment, crops ~1 m, grass ~0.7 m, bridge deck 4.5 m wide (stretched to the river).
The generators keep their own modelling unit (`MODEL_TILE` in config.js), so rebuilding a library does not change sizes. Field numbers are metres or nodes as commented in `field.js` / `constants.js`:
rivers are 5-9.5 m wide (width 0.5-0.95 nodes), paths 3 m (0.3), bridges = river + 4 m (so ~9-13 m), hills 1.5 m, banks slope down over 2 m, water level -0.5 m. Because a river is narrower than a node,
routing keeps `WATER_MARGIN` (0.5 node) from water and the walkable strip of a bridge reaches `BRIDGE_SLACK` past its ends.
Squads: soldiers are modelled 0.3 units tall; `SOLDIER_SCALE` (6) makes them a real 1.8 m, `SQUAD_EXAGGERATION` (2.5) draws the squad bigger so it can be seen (1 = true scale). The orange route
arrow is an overlay (3 m wide). Camera, fog, shadow and ground detail are scaled by the same factor in `stage.js` / `ground_view.js`.

## Pines (assets/pine.glb): the only tree

`tools/build_tree.mjs` strips the product-shot scene out of the downloaded `assets/tree.glb`: it **keeps** the pine and its own pebbles (`pine_stone`), grass tufts (`pine_grass`) and gold sparkles
(`pine_glitter`, emissive), and **drops** the presentation (ground plane, light-ray cards, the second scene copy, the needles' self-lighting and 3.7 MB normal map, the leaf rosette on top
unless `--keep-apex`: its dark leaves read as a black spike). Node transforms are baked, each material is one mesh, the needle map is shrunk, and it is scaled to `--height` m (18) with the trunk on y = 0.
`src/map/tree_asset.js` loads it and registers one asset, `pine` (the field emits only `pine`: one height, a few percent of natural variation per tree; `TREE.height` in config.js).
The full pine is 27k triangles and its pebbles 20k, so `props_view.js` draws the full model only on the `TREE.detailCap` nearest pines within `TREE.detailRadius` nodes of the camera target,
the pebbles / grass / sparkles only on the nearest `TREE.extrasCap` within `TREE.extrasRadius`, and everything else as `far`, a ~130-triangle cone stack built from the model's measured tiers.
Replacing the tree = drop a new `tree.glb`, rerun the tool (node names `A1_Pine*` etc., see the regexes at the top of the tool). Oaks and bushes were removed from the library; the small procedural
`pine` builder in `tools/mapkit/trees.mjs` still decorates outcrops and towers.

## Lighting panel

Top bar **Light**: presets Default (the original look), Ambient (soft even fill, weak sun) and Warm (low golden sun, long shadows), plus sliders for ambient, sky fill, sun, warmth, exposure, sun
angle / height and a shadows switch. Code: `LIGHT_PRESETS` and `stage.lighting` in `src/map/stage.js` (`preset(name)`, `set(patch)`, `onChange`); URL `?light=Warm&panel`.

## Team colours

Flags, tunics, shield rims, crests, banners and the squad's parade ring are exported in materials `team_a` (primary) / `team_b` (secondary)
(`_ds` = double-sided cloth) with neutral grey vertex colours. `TeamSkins.apply(root, team)` gives them the team's materials;
edit `team.primary` / `team.secondary` and call `skins.refresh(team)` to recolour every castle and squad of that team at once. The tower's banners use the `baron` team.

## Map interaction and scripting

Top bar: kingdom picker (frames it and rings its castles), map choice (sheet example or a generated seed), Iso/Top/Side, Grid, Blocked (what the pathfinder
sees), Dev (send attacks by form, edit team colours, list marches). Click a castle, then a castle or NPC, to send an attack using the Dev panel's seconds.
From code: `window.mapApi` (`selectKingdom`, `loadKingdom`, `sendAttack`, `setMapSpec`, `setNow`, `refresh`, ...), or URL options for screenshots:
`world.html?kingdom=red&send=3,3>46,46:40;46,3>3,46:60&advance=15&view=Top&gen=5&frame=0,0,50,50&zoom=0.6&dev&shot`, and `?stress=3000` for a 1286-node map with 3000 castles.
