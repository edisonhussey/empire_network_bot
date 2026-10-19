// Solid shapes (circles and oriented boxes, in nodes) in a bucket hash, with distance queries. Rocks, buildings, fields and trees are solids;
// the pathfinder asks "how far is the nearest solid" and keeps its clearance.
const BUCKET = 8;
const key = (bx, by) => bx * 100003 + by;

export function distanceTo(s, x, y) {
  if (s.shape === 'circle') return Math.max(0, Math.hypot(x - s.x, y - s.y) - s.r);
  const c = Math.cos(s.rot), n = Math.sin(s.rot), dx = x - s.x, dy = y - s.y;
  const lx = dx * c + dy * n, ly = -dx * n + dy * c;
  return Math.hypot(Math.max(0, Math.abs(lx) - s.hx), Math.max(0, Math.abs(ly) - s.hy));
}

export class SolidIndex {
  constructor() { this.cells = new Map(); this.count = 0; }

  add(s) {
    const reach = (s.shape === 'circle' ? s.r : Math.hypot(s.hx, s.hy)) + 3;
    for (let by = Math.floor((s.y - reach) / BUCKET); by <= Math.floor((s.y + reach) / BUCKET); by++) {
      for (let bx = Math.floor((s.x - reach) / BUCKET); bx <= Math.floor((s.x + reach) / BUCKET); bx++) (this.cells.get(key(bx, by)) ?? this.cells.set(key(bx, by), []).get(key(bx, by))).push(s);
    }
    this.count++;
    return s;
  }

  /** Distance to the nearest solid (optionally of one `kind`), capped at 3 nodes (Infinity beyond). */
  nearest(x, y, kind = null) {
    const list = this.cells.get(key(Math.floor(x / BUCKET), Math.floor(y / BUCKET)));
    if (!list) return Infinity;
    let best = Infinity;
    for (const s of list) { if (kind && s.kind !== kind) continue; const d = distanceTo(s, x, y); if (d < best) best = d; }
    return best;
  }
}
