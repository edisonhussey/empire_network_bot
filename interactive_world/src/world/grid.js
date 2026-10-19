// The walkability grid: one flag per node. Out-of-range nodes count as blocked. Pure data, no rendering.
export class Grid {
  constructor(size, pad) {
    this.min = -pad;
    this.max = size + pad;
    this.n = this.max - this.min + 1;
    this.cells = new Uint8Array(this.n * this.n);
  }

  inBounds(x, y) {
    return x >= this.min && x <= this.max && y >= this.min && y <= this.max;
  }

  index(x, y) {
    return (y - this.min) * this.n + (x - this.min);
  }

  isBlocked(x, y) {
    return !this.inBounds(x, y) || this.cells[this.index(x, y)] === 1;
  }

  block(x, y) {
    if (this.inBounds(x, y)) this.cells[this.index(x, y)] = 1;
  }

  /** Block the w x h nodes whose top-left (min x, min y) node is (x, y). */
  blockBox(x, y, w, h) {
    for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) this.block(x + i, y + j);
  }

  /** Free the same nodes (a bridge carved through water). */
  unblockBox(x, y, w, h) {
    for (let j = 0; j < h; j++) {
      for (let i = 0; i < w; i++) if (this.inBounds(x + i, y + j)) this.cells[this.index(x + i, y + j)] = 0;
    }
  }

  /**
   * Block the box plus a rounded margin: every node within `r` of the box's rectangle (the box owns the unit squares round its nodes).
   * Corners therefore curve like a circle of radius r instead of squaring off.
   */
  blockRounded(x, y, w, h, r) {
    const rx0 = x - 0.5, ry0 = y - 0.5, rx1 = x + w - 0.5, ry1 = y + h - 0.5;
    const r2 = r * r;
    for (let ny = Math.max(this.min, Math.floor(ry0 - r)); ny <= Math.min(this.max, Math.ceil(ry1 + r)); ny++) {
      for (let nx = Math.max(this.min, Math.floor(rx0 - r)); nx <= Math.min(this.max, Math.ceil(rx1 + r)); nx++) {
        const dx = Math.max(rx0 - nx, 0, nx - rx1), dy = Math.max(ry0 - ny, 0, ny - ry1);
        if (dx * dx + dy * dy <= r2 + 1e-9) this.cells[this.index(nx, ny)] = 1;
      }
    }
  }

  /** Every blocked node as [x, y] within an optional rect (debug overlay). */
  blockedNodes(rect) {
    const out = [];
    const x0 = Math.max(this.min, rect ? Math.floor(rect.x0) : this.min), x1 = Math.min(this.max, rect ? Math.ceil(rect.x1) : this.max);
    const y0 = Math.max(this.min, rect ? Math.floor(rect.y0) : this.min), y1 = Math.min(this.max, rect ? Math.ceil(rect.y1) : this.max);
    for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) if (this.cells[this.index(x, y)]) out.push([x, y]);
    return out;
  }
}
