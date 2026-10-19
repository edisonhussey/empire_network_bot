// Routes for DRAWING marches. A route is never used for timing: a march always takes its given duration, whatever the route looks like.
// The router builds a walkability grid from the terrain (rounded margins round every solid) and the castle / NPC footprints, runs A*
// between the two points (from the edge of a footprint to the edge of the other), straightens the result and rounds its corners.
// It is lazy and cached, with a search budget: on a huge map or a walled-in castle it degrades to a straight line (route.approximate).
import { Grid } from './grid.js';
import { FieldGrid } from './fieldgrid.js';
import { findPath } from './pathfinding.js';
import { Polyline } from './polyline.js';
import { roundCorners } from './curve.js';
import { ROUTE } from './config.js';

const SQRT2 = Math.SQRT2;

export class Router {
  constructor(world) {
    this.world = world;
    this.cache = new Map();
    this.grid = null;
    this.builtFor = -1;
  }

  /** The grid the pathfinder sees (rebuilt when the world changes). Exposed for the debug overlay. */
  get walkGrid() {
    const w = this.world;
    if (this.builtFor !== w.version) {
      const g = new Grid(w.size, w.pad);
      w.terrain.compile(g, ROUTE.extraClearance);
      for (const e of w.allEntities()) { const h = w.footprintHalf(e); g.blockBox(e.x - h, e.y - h, 2 * h + 1, 2 * h + 1); }
      this.base = g;
      this.grids = [];
      this.grid = w.field ? this.#levelGrid(0) : g;
      this.builtFor = w.version;
      this.cache.clear();
    }
    return this.grid;
  }

  /** The grid for relaxation level `i` (0 = strict clearances), built on first use. */
  #levelGrid(i) {
    if (!this.grids[i]) this.grids[i] = new FieldGrid(this.base, this.world.field, { clearance: ROUTE.levels[i].clearance, treeClearance: ROUTE.levels[i].tree });
    return this.grids[i];
  }

  #free(grid, p, maxR = 8) {
    const x = Math.round(p.x), y = Math.round(p.y);
    if (!grid.isBlocked(x, y)) return { x, y };
    for (let r = 1; r <= maxR; r++) {
      let best = null, bd = Infinity;
      for (let dy = -r; dy <= r; dy++) for (let dx = -r; dx <= r; dx++) {
        if (Math.max(Math.abs(dx), Math.abs(dy)) !== r || grid.isBlocked(x + dx, y + dy)) continue;
        const d = dx * dx + dy * dy;
        if (d < bd) { bd = d; best = { x: x + dx, y: y + dy }; }
      }
      if (best) return best;
    }
    return null;
  }

  /** Start / goal nodes for an endpoint: the ring just outside a footprint, or the point itself. */
  #nodes(grid, p) {
    const e = this.world.entityAt(p.x, p.y);
    const half = e ? this.world.footprintHalf(e) : 0;
    if (!half) { const f = this.#free(grid, p); return { nodes: f ? [f] : [], half: 0 }; }
    const ring = half + 1, nodes = [];
    for (let dy = -ring; dy <= ring; dy++) for (let dx = -ring; dx <= ring; dx++) {
      if (Math.max(Math.abs(dx), Math.abs(dy)) === ring && !grid.isBlocked(e.x + dx, e.y + dy)) nodes.push({ x: e.x + dx, y: e.y + dy });
    }
    if (!nodes.length) { const f = this.#free(grid, { x: e.x, y: e.y }, ring + 8); if (f) nodes.push(f); }
    return { nodes, half };
  }

  /** Pull the first and last point onto the footprint edge of the castle / NPC at each end, so a squad starts and ends at the building itself. */
  #toEdges(points, from, to) {
    const out = points.slice();
    const snap = (i, p) => {
      const e = this.world.entityAt(p.x, p.y);
      if (!e) return;
      const q = out[i], half = this.world.footprintHalf(e) + 0.5 - ROUTE.edgeInset;
      const d = Math.max(Math.abs(q.x - e.x), Math.abs(q.y - e.y));
      if (d > half) out[i] = { x: e.x + ((q.x - e.x) * half) / d, y: e.y + ((q.y - e.y) * half) / d };
    };
    snap(0, from); snap(out.length - 1, to);
    return out;
  }

  /** { from, to, points, polyline, length (nodes), approximate } for a march between two points (cached). */
  route(from, to) {
    const grid = this.walkGrid;
    const key = `${from.x},${from.y}>${to.x},${to.y}`;
    const hit = this.cache.get(key);
    if (hit) { this.cache.delete(key); this.cache.set(key, hit); return hit; }
    let points = null, approximate = false, relaxed = 0;
    const levels = this.world.field ? ROUTE.levels.length : 1;
    for (let level = 0; level < levels && !points; level++) {
      const grid = this.world.field ? this.#levelGrid(level) : this.walkGrid;
      const a = this.#nodes(grid, from), b = this.#nodes(grid, to);
      if (!a.nodes.length || !b.nodes.length) continue;
      const found = findPath(grid, { starts: a.nodes, goals: b.nodes, aim: { x: to.x, y: to.y, slack: (b.half + 1) * SQRT2 }, maxExpansions: ROUTE.maxExpansions });
      if (found?.nodes) { points = roundCorners(found.nodes, ROUTE.cornerRadius); relaxed = level; }
      else if (found?.exceeded) break;   // the budget is gone: relaxing would only search more
    }
    if (!points) { points = [{ x: from.x, y: from.y }, { x: to.x, y: to.y }]; approximate = true; }
    else points = this.#toEdges(points, from, to);
    const polyline = new Polyline(points);
    const route = { from, to, points, polyline, length: polyline.length, approximate, relaxed };
    this.cache.set(key, route);
    if (this.cache.size > ROUTE.cacheSize) this.cache.delete(this.cache.keys().next().value);
    return route;
  }
}
