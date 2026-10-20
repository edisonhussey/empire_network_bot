// Layer 1, the quantized tile terrain (1 m logical grid, 0.2 m height steps). Rules: architecture/TILE_TERRAIN_ARCHITECTURE.md.
//
// A TileGrid is `width` x `depth` tiles of 1 m. Like the 5 m TerrainGrid, every tile owns its own four corner heights, in the same fixed
// order [TR, BR, BL, TL] (top = north = -Z, right = east = +X), and neighbours may disagree (that is how risers and cliffs exist).
// Unlike TerrainGrid, heights are whole LEVELS (Int16, 1 level = STEP = 0.2 m), and inside one tile the corners differ by at most one
// level, so every tile is one of exactly five shapes (tile_shapes.js). A 5 m terrain cell is 5 x 5 tiles.
//
// Side data, indexed like the tiles (index = tz * width + tx):
//   ground: Uint8Array   GROUND.* (what the top is: grass, path, rock, sand, stream bed, pad)
//   water:  Int16Array   water surface in levels above the tile, or NO_WATER
import { TR, BR, BL, TL, CORNERS, CORNER_OFFSET } from '../terrain/grid.js';

export { TR, BR, BL, TL, CORNERS, CORNER_OFFSET };
export const TILE = 1;          // metres per tile
export const STEP = 0.2;        // metres per height level
export const NO_WATER = -32768;
export const GROUND = { grass: 0, path: 1, rock: 2, sand: 3, bed: 4, pad: 5 };
export const GROUND_NAMES = Object.keys(GROUND);

export class TileGrid {
  constructor({ width = 48, depth = 40 } = {}) {
    this.width = width;
    this.depth = depth;
    this.levels = new Int16Array(width * depth * 4);
    this.ground = new Uint8Array(width * depth);
    this.water = new Int16Array(width * depth).fill(NO_WATER);
  }

  get tileCount() { return this.width * this.depth; }
  inBounds(tx, tz) { return tx >= 0 && tz >= 0 && tx < this.width && tz < this.depth; }
  index(tx, tz) { return tz * this.width + tx; }

  /** Live view of the four corner levels [TR, BR, BL, TL]. */
  corners(tx, tz) { const i = this.index(tx, tz) * 4; return this.levels.subarray(i, i + 4); }
  setCorners(tx, tz, c) { this.levels.set(c, this.index(tx, tz) * 4); }
  /** Corner height in metres. */
  cornerHeight(tx, tz, k) { return this.levels[this.index(tx, tz) * 4 + k] * STEP; }
  /** Lowest corner level of a tile (where the tile mesh's y = 0 goes). */
  base(tx, tz) { const c = this.corners(tx, tz); return Math.min(c[0], c[1], c[2], c[3]); }
  /** Highest minus lowest corner, in levels: 0 or 1 for a valid tile. */
  spread(tx, tz) { const c = this.corners(tx, tz); return Math.max(c[0], c[1], c[2], c[3]) - Math.min(c[0], c[1], c[2], c[3]); }

  /** World x, z of a corner. */
  cornerXZ(tx, tz, k) { return [tx + CORNER_OFFSET[k][0], tz + CORNER_OFFSET[k][1]]; }
  tileAt(x, z) { const tx = Math.floor(x), tz = Math.floor(z); return this.inBounds(tx, tz) ? { tx, tz } : null; }

  waterHeight(tx, tz) { const w = this.water[this.index(tx, tz)]; return w === NO_WATER ? null : w * STEP; }

  /** Every tile within one level? Returns the first bad tile or null. */
  firstInvalid() {
    for (let tz = 0; tz < this.depth; tz++) for (let tx = 0; tx < this.width; tx++) if (this.spread(tx, tz) > 1) return { tx, tz };
    return null;
  }
}
