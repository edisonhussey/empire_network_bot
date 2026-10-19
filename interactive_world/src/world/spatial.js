// Bucketed spatial index over points: window queries cost the size of the window, not the size of the map.
export class SpatialIndex {
  constructor(bucket = 8) {
    this.bucket = bucket;
    this.cells = new Map();
  }

  #key(bx, by) {
    return bx * 100003 + by;
  }

  insert(item) {
    const k = this.#key(Math.floor(item.x / this.bucket), Math.floor(item.y / this.bucket));
    let cell = this.cells.get(k);
    if (!cell) this.cells.set(k, (cell = new Set()));
    cell.add(item);
  }

  remove(item) {
    this.cells.get(this.#key(Math.floor(item.x / this.bucket), Math.floor(item.y / this.bucket)))?.delete(item);
  }

  /** Items whose point lies inside the rect. */
  query(rect) {
    const out = [];
    const b = this.bucket;
    for (let by = Math.floor(rect.y0 / b); by <= Math.floor(rect.y1 / b); by++) {
      for (let bx = Math.floor(rect.x0 / b); bx <= Math.floor(rect.x1 / b); bx++) {
        const cell = this.cells.get(this.#key(bx, by));
        if (!cell) continue;
        for (const it of cell) if (it.x >= rect.x0 && it.x <= rect.x1 && it.y >= rect.y0 && it.y <= rect.y1) out.push(it);
      }
    }
    return out;
  }
}
