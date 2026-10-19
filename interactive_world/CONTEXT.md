# interactive_world: context and summary

> **New base (2026-10-19):** fresh work goes in `core/`, ruled by `architecture/` (start with `architecture/README.md`). The terrain is now per-cell corner heights with reconstruction walls (`core/`, http://127.0.0.1:5174/core/). Everything below describes the previous map prototype in `src/`, kept for reference; `core/` does not import it.


Read this first when resuming. It records what this prototype is for, the decisions already made, the contracts between modules, and what is
deliberately NOT built yet. `README.md` has the commands and the file map.

## Goal

A cartoon-stylised 3D view of the Goodgame-Empire-style world: kingdoms with castles at x,y, NPC sites (dungeons / robber baron towers),
a 3D map (boulders, rivers with bridges), and marching attacks drawn between them. It must run inside a tab of the desktop app
(`rust_distributable`, a Tauri webview) later; for now it is a standalone prototype served by `node serve.mjs`.
This is a visual layer: the game rules live elsewhere.

## Decisions (made by the owner; do not re-litigate)

1. **Logic is coordinates only.** An attack is `from [x,y] -> to [x2,y2]`, `duration` seconds, `startedAt`. Arrival time = `startedAt + duration`,
   nothing else. Progress = `(now - startedAt) / duration`, rendered as a fraction of the way along a drawn route.
2. **Routes are visual only.** A* with obstacle boxes, a rounded clearance margin so squads never graze obstacles, then straightened and
   corner-rounded. The route never influences timing, so terrain details cannot change when attacks land. It is best effort (budgeted;
   falls back to a straight line, `route.approximate`).
3. **Kingdoms own data**: `{ id, name, team, castles: [[x,y],...], npc: [[x,y],...] }`, preloaded (demo scenario now, an API later).
   A kingdom is selected from the top bar or from outside through `focus.selectKingdom(id)` / `app.selectKingdom(id)`.
4. **Render only what is in the window** (what the camera sees, plus a margin, capped). Everything is pooled/instanced; far castles are cheap
   proxies, far marches are dots, boulders are instanced. Never instantiate the whole map. Target scale: the real map (1286 x 1286, thousands
   of castles).
5. **Modular, mountable.** World rules are pure JS (run in node, tested). The map app mounts into any container and is driven by an API,
   so it can become a tab. No database yet.
6. **Orange cartoon arrow** shows each march's remaining route; hovering a target after picking a source previews it.

## Contracts

World data (`src/world/world.js`):
* `new World({ size })`; `addKingdom({ id, name, team, castles, npc })` (replaces same id); `terrain.add({ kind, x, y, w, h })`.
  Boxes are `{x, y, w, h}` = top-left node and size in nodes. Terrain kinds: `boulder`, `wall`, `river` (water), `bridge` (carved through water).
* `setField(spec | MapField)` attaches the land (castles and NPCs level the ground they stand on); `heightAt(x, y)` / `walkHeightAt(x, y)` read it. Spec and field: README, "The land".
* `sendMarch({ from, to, duration, startedAt?, kingdomId?, kind?, units? })`; `setNow(t)` / `advance(dt)`; events `march`, `arrive`, `kingdom`, `team`.
* `queryWindow(rect)` -> `{ castles, npcs }` (spatial index, cost = window size); `entityAt(x, y)`; `kingdomBounds(id)`; `marchesInWindow(rect)`.
* Castle/NPC footprint is 3x3 nodes; NPC kinds are registered in `npc_kinds.js` (`registerNpcKind(kind, { label, footprintHalf })`).

Map app (`src/map/app.js`): `createMapApp({ container, assets, world?, autoClock? })` -> `{ world, focus, teams, selectKingdom, loadKingdom,
removeKingdom, sendAttack, setNow, setWindow, frame, simulate, dispose }`. Set `autoClock:false` and feed `setNow(serverSeconds)` to follow a server clock.

Team colours: `team_a` / `team_b` materials in the GLBs are tinted from `teams[id].primary/secondary` (`TeamSkins`); edit a team and call
`skins.refresh` (the app's UI does this). Roofs and stone are fixed colours.

## Built from the owner's reference sheets

* **Robber baron tower** (`robber_baron_castle.png`): `tools/tower/tower.mjs`, parametric (`segments` = height), rock mound, arched door, timber bands,
  battlement, banners (team-tinted, `baron` team), ladder, pines. Registered as the `robber_baron` NPC kind and drawn by `npc_view.js`.
* **Map pieces** (`map_assets_v1.png`): 26 individually built, origin-centred pieces in `assets/map_library.glb` (see README); a generator and a hand-authored example
  share one spec format, so the sheet's example map and any seeded map are made of the same parts. The keep on the sheet is swapped for our own castle.
* **Land and attacks**: rivers with flowing water and stone banks, dirt roads, forests, farms with fenced fields, houses, watchtowers, cliffs, rocks, bridges, heights;
  attacks route over it (roads preferred, bridges, forests avoided) and are drawn with the orange arrow at the land's height.

## Still placeholder or open

* **Scale** is metres now (README "Scale"): node = 10 m, castle / tower 24 m, pine 18 m, rivers 5-9.5 m, paths 3 m, bridges ~10 m. Squads are drawn 2.5x true size on purpose (`SQUAD_EXAGGERATION`). Rivers, houses etc. are placeholders sized by common sense, not by the sheet.
* **Trees**: the pine is the only tree (oaks / bushes removed until real models arrive). Its dark apex rosette is left out (`--keep-apex` to restore).

* **`dungeon` NPC kind**: a plain marker (kept for kinds without art). Other NPC kinds are added in `npc_kinds.js` + `registerNpcRenderer`.
* **Terrain art not yet matched to the sheet**: cliffs are columnar blocks, the river has no waterfalls/foam, houses have no smoke or animals, trees do not sway.
  Slope pieces are visual only (walkable, no height change); cliffs block but do not change the ground height.
* **Attack types, unit counts, sieges**: a march carries `kind` and `units` but only draws the standard squad.
* **Embedding in `rust_distributable`**: not done (that project is separate and must not be touched from here without its docs updated). The path:
  serve `src/`, `assets/`, `vendor/` as the tab's static files, call `createMapApp({ container, assets })`, push kingdoms with `loadKingdom`, the land with
  `setMapSpec`, attacks with `sendAttack`.

## Performance notes

* Window query: bucketed index, ~0.001 ms for a 60x60 window on a 3000-castle map. A* uses stamp-based scratch buffers (no per-route allocation)
  and a 60k-expansion budget; a long route on the large map is ~20 ms. Routes are computed only for marches that get a full squad
  (2 new per sync), cached (LRU 600), and the cache clears when castles / NPCs / terrain change.
* LOD caps live in `config.js` (`LOD`, `WINDOW`): 40 full castles, 14 animated squads, 40 NPC models; extras fall back to proxies / dots. Ground is built in 12-node chunks at three
  detail levels (2 per frame), grass tufts only within ~42 nodes of the camera target, props are instanced (one draw call per piece and material).
* Outline hulls double the triangle count of full models; lower the caps first on weak GPUs.

## Verification

`node tools/test_world.mjs` (28 tests: A*, clearance, bridges, every castle reachable on the sheet map and on generated seeds, timing independent of routes, window queries,
road preference, forest fallback, a 1286-node land and a 3000-castle map). Visual checks were done with headless Chrome screenshots (`world.html?kingdom=blue&send=...&advance=12&shot`, `?gen=5`, `?stress=3000`) and a console-error sweep of both pages;
the click flow (select source, click target) and real-GPU frame rates were not exercised.
