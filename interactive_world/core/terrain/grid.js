// Layer 1, logical terrain data. A TerrainGrid is `width` x `depth` cells, each `cellSize` metres square, and every cell stores its own four
// corner heights (metres). Cells are independent: neighbours do not have to agree on the heights of the corners they share.
//
// Corner order (CellSurface) is fixed everywhere: [top-right, bottom-right, bottom-left, top-left].
// "Top" is north = -Z, "right" is east = +X (looking down on the map with +X right and +Z towards the viewer's bottom edge).
// Cell (cx, cz) covers x in [cx * cellSize, (cx + 1) * cellSize] and z in [cz * cellSize, (cz + 1) * cellSize].
//
// Storage is one Float32Array of width * depth * 4 heights (16 bytes per cell): no per-cell objects, cheap to copy, send to a worker or save.

export const TR = 0, BR = 1, BL = 2, TL = 3;
export const CORNERS = ['top-right', 'bottom-right', 'bottom-left', 'top-left'];

/** Corner offsets in cell units [dx, dz] from the cell's top-left (min x, min z), in CellSurface order. */
export const CORNER_OFFSET = [[1, 0], [1, 1], [0, 1], [0, 0]];

export class TerrainGrid {
  /** @param {{ width?: number, depth?: number, cellSize?: number }} opts */
  constructor({ width = 100, depth = 100, cellSize = 5 } = {}) {
    this.width = width;
    this.depth = depth;
    this.cellSize = cellSize;
    this.heights = new Float32Array(width * depth * 4);
  }

  get cellCount() { return this.width * this.depth; }
  inBounds(cx, cz) { return cx >= 0 && cz >= 0 && cx < this.width && cz < this.depth; }
  index(cx, cz) { return cz * this.width + cx; }

  /** The cell's four heights as a live view (writes go straight into the grid). Order: TR, BR, BL, TL. */
  surface(cx, cz) { const i = this.index(cx, cz) * 4; return this.heights.subarray(i, i + 4); }

  /** One corner height. */
  corner(cx, cz, k) { return this.heights[this.index(cx, cz) * 4 + k]; }

  /** Set a cell from a CellSurface [TR, BR, BL, TL]. */
  setSurface(cx, cz, s) { this.heights.set(s, this.index(cx, cz) * 4); }

  /** Plain-object copy of one cell (the TerrainCell shape: { surface: [TR, BR, BL, TL] }). */
  cell(cx, cz) { return { surface: Array.from(this.surface(cx, cz)) }; }

  /** Highest minus lowest corner of a cell: 0 = flat, the spec's usual limit is 0.2 m. */
  spread(cx, cz) { const s = this.surface(cx, cz); return Math.max(s[0], s[1], s[2], s[3]) - Math.min(s[0], s[1], s[2], s[3]); }

  /** World-space x, z of a corner. */
  cornerXZ(cx, cz, k) { return [(cx + CORNER_OFFSET[k][0]) * this.cellSize, (cz + CORNER_OFFSET[k][1]) * this.cellSize]; }

  /** Cell containing world point (x, z), or null outside the map. */
  cellAt(x, z) {
    const cx = Math.floor(x / this.cellSize), cz = Math.floor(z / this.cellSize);
    return this.inBounds(cx, cz) ? { cx, cz } : null;
  }

  /** Lowest and highest height anywhere in the grid. */
  range() {
    let lo = Infinity, hi = -Infinity;
    for (const h of this.heights) { if (h < lo) lo = h; if (h > hi) hi = h; }
    return { min: lo, max: hi };
  }
}
