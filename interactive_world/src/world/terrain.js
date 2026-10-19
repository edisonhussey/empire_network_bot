// Map features as boxes, the only thing the pathfinder knows about the land: boulders, rivers (water), bridges (carved through water), walls.
// A feature is { kind, x, y, w, h } (top-left node and size in nodes). Each kind says whether it blocks and how much margin it keeps.
// Rendering of each kind is the renderer's business (src/map/terrain_view.js); add a kind here and a drawer there.
import { CLEARANCE } from './config.js';

export const KINDS = {
  boulder: { solid: true, clearance: CLEARANCE.boulder },
  wall: { solid: true, clearance: CLEARANCE.wall },
  river: { solid: true, clearance: CLEARANCE.river },   // water: no margin, squads may walk along the bank
  bridge: { carve: true },                               // frees the nodes of the water it crosses
};

export class Terrain {
  constructor() {
    this.features = [];
    this.version = 0;
  }

  add(feature) {
    if (!KINDS[feature.kind]) throw new Error(`unknown terrain kind "${feature.kind}"`);
    const f = { w: 1, h: 1, ...feature };
    f.id ??= `${f.kind}:${f.x},${f.y}`;
    this.features.push(f);
    this.version++;
    return f;
  }

  addMany(list) {
    for (const f of list) this.add(f);
  }

  ofKind(kind) {
    return this.features.filter((f) => f.kind === kind);
  }

  /** Mark the grid: solids with their rounded margins (+ `extra`), then bridges carved through. */
  compile(grid, extra = 0) {
    for (const f of this.features) {
      const k = KINDS[f.kind];
      if (k.solid) grid.blockRounded(f.x, f.y, f.w, f.h, (k.clearance ?? CLEARANCE.default) + (k.clearance ? extra : 0));
    }
    for (const f of this.features) if (KINDS[f.kind].carve) grid.unblockBox(f.x, f.y, f.w, f.h);
  }
}
