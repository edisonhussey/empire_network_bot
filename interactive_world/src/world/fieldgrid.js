// The pathfinder's view of a MapField: the same interface as Grid, but nodes are evaluated lazily (and remembered) the first time the search
// touches them, so a 1300 x 1300 map costs only what each route actually explores. `base` is the eager grid of box terrain and castle footprints.
export class FieldGrid {
  constructor(base, field, { clearance = 0.9, treeClearance = 0.5 } = {}) {
    this.base = base;
    this.field = field;
    this.clearance = clearance;
    this.treeClearance = treeClearance;
    this.min = base.min; this.max = base.max; this.n = base.n;
    this.cells = { length: base.cells.length };
    this.known = new Uint8Array(base.cells.length);    // 0 unknown, 1 free, 2 blocked
    this.minCost = field.roads.length ? 0.6 : 1;
  }

  inBounds(x, y) { return this.base.inBounds(x, y); }
  index(x, y) { return this.base.index(x, y); }

  isBlocked(x, y) {
    if (!this.base.inBounds(x, y)) return true;
    const i = this.base.index(x, y);
    let v = this.known[i];
    if (!v) { v = this.base.isBlocked(x, y) || this.field.blockedAt(x, y, this.clearance, this.treeClearance) ? 2 : 1; this.known[i] = v; }
    return v === 2;
  }

  costAt(x, y) { return this.field.costAt(x, y); }

  /** Blocked nodes inside a rect as [x, y] (debug overlay), evaluated on demand. */
  blockedNodes(rect) {
    const out = [];
    for (let y = Math.max(this.min, Math.floor(rect.y0)); y <= Math.min(this.max, Math.ceil(rect.y1)); y++) {
      for (let x = Math.max(this.min, Math.floor(rect.x0)); x <= Math.min(this.max, Math.ceil(rect.x1)); x++) if (this.isBlocked(x, y)) out.push([x, y]);
    }
    return out;
  }
}
