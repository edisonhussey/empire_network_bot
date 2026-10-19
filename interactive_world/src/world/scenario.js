// A demo map as plain data (swap for loaded data later). Kingdoms own castles as [[x, y], ...] and NPCs as [[x, y], ...];
// terrain features are boxes { kind, x, y, w, h } in nodes. A river splits the map with two bridges; boulder walls make routes bend.
export const DEMO = {
  size: 50,
  kingdoms: [
    { id: 'blue', name: 'Blue Kingdom', team: 'blue', castles: [[3, 3], [9, 6], [5, 12], [14, 3]], npc: [{ x: 18, y: 12, kind: 'robber_baron' }] },
    { id: 'red', name: 'Red Kingdom', team: 'red', castles: [[46, 46], [40, 43], [44, 36], [34, 47]], npc: [{ x: 28, y: 40, kind: 'robber_baron' }] },
    { id: 'green', name: 'Green Kingdom', team: 'green', castles: [[46, 3], [40, 8], [45, 14]], npc: [{ x: 38, y: 18, kind: 'robber_baron' }] },
    { id: 'gold', name: 'Gold Kingdom', team: 'gold', castles: [[3, 46], [9, 41], [4, 35]], npc: [{ x: 12, y: 30, kind: 'robber_baron' }] },
    { id: 'purple', name: 'Purple Kingdom', team: 'purple', castles: [[25, 25], [21, 29]], npc: [{ x: 28, y: 19, kind: 'robber_baron' }, { x: 25, y: 33, kind: 'robber_baron' }] },
  ],
  terrain: [
    { kind: 'river', x: 31, y: -4, w: 2, h: 60 },         // runs the whole height of the map, through the padding rows too
    { kind: 'bridge', x: 31, y: 11, w: 2, h: 5 },
    { kind: 'bridge', x: 31, y: 32, w: 2, h: 5 },
    { kind: 'boulder', x: 8, y: 20, w: 14, h: 2 },
    { kind: 'boulder', x: 14, y: 36, w: 2, h: 8 },
    { kind: 'boulder', x: 34, y: 22, w: 12, h: 2 },
    { kind: 'boulder', x: 36, y: 36, w: 2, h: 6 },
    { kind: 'boulder', x: 20, y: 8, w: 3, h: 3 },
    { kind: 'boulder', x: 6, y: 26, w: 4, h: 3 },
    { kind: 'boulder', x: 44, y: 28, w: 2, h: 2 },
    { kind: 'boulder', x: 26, y: 44, w: 3, h: 2 },
    { kind: 'boulder', x: 18, y: 44, w: 2, h: 2 },
    { kind: 'boulder', x: 22, y: 16, w: 2, h: 2 },
  ],
};

import { SHEET_SPEC } from './mapfield/sheet_example.js';
import { generateMap } from './mapfield/generate.js';

/** The demo kingdoms on the hand-authored sheet map (river, bridges, roads, forests, farms, towers). */
export const SHEET = { size: 50, kingdoms: DEMO.kingdoms, map: SHEET_SPEC };

/** The same kingdoms on a generated map: any seed gives a different land with the same kinds of pieces. */
export const generated = (seed = 1, options = {}) => ({
  size: 50, kingdoms: DEMO.kingdoms,
  map: generateMap({ size: 50, seed, castles: DEMO.kingdoms.flatMap((k) => k.castles), npcs: DEMO.kingdoms.flatMap((k) => k.npc), ...options }),
});

/** Fill a World from scenario data. */
export function loadScenario(world, data) {
  if (data.map) world.setField(data.map);                 // a MapField spec: rivers, roads, forests, buildings...
  for (const k of data.kingdoms) world.addKingdom(k);
  world.terrain.addMany(data.terrain ?? []);
  return world;
}
