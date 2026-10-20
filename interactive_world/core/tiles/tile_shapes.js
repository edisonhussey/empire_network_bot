// The five tile shapes and the one surface rule for tiles, shared by the mesh builder, the height query and the Blender kit (terrain_kit.py).
//
// Subtract a tile's lowest corner level from its four corners and each corner is 0 or 1. Up to rotation there are exactly five patterns:
//
//   shape         canonical [TR, BR, BL, TL]   what it is                      rotations
//   flat          [0, 0, 0, 0]                 level ground                    1
//   corner_high   [1, 0, 0, 0]                 one corner raised (north-east)  4
//   corner_low    [0, 1, 1, 1]                 one corner lowered (north-east) 4
//   ramp          [1, 0, 0, 1]                 rises one step to the north     4
//   saddle        [1, 0, 1, 0]                 two opposite corners raised     2
//
// The canonical tile is authored in Blender facing north (Blender +Y = game -Z, architecture/ASSET_GLB_PROTOCOL.md). A tile with rotation r
// is the canonical tile turned r quarter turns counter-clockwise seen from above (three.js rotation.y = r * PI / 2, the same table as
// FACING_ROTATION: r = 0 N, 1 W, 2 S, 3 E). One quarter turn moves corner k to (k + 3) % 4 (TR -> TL -> BL -> BR -> TR).
//
// Surface rule: the top is the BILINEAR patch through the four corners, sampled on a 4 x 4 grid of sub-quads (SUBDIV). heightAt below is
// the same bilinear formula; the drawn sub-quad triangles deviate from it by at most STEP * (1/SUBDIV)^2 / 4 (about 3 mm).
import { TR, BR, BL, TL, STEP } from './tile_grid.js';

export const SUBDIV = 4;
export const SHAPES = ['flat', 'corner_high', 'corner_low', 'ramp', 'saddle'];
export const CANONICAL = {
  flat: [0, 0, 0, 0],
  corner_high: [1, 0, 0, 0],
  corner_low: [0, 1, 1, 1],
  ramp: [1, 0, 0, 1],
  saddle: [1, 0, 1, 0],
};
export const ROTATION_NAMES = ['N', 'W', 'S', 'E'];

/** The canonical pattern turned r quarter turns counter-clockwise (seen from above). */
export function rotatePattern(p, r) {
  let out = p.slice();
  for (let i = 0; i < (r & 3); i++) { const n = [0, 0, 0, 0]; for (let k = 0; k < 4; k++) n[(k + 3) % 4] = out[k]; out = n; }
  return out;
}

// pattern bits (TR=1, BR=2, BL=4, TL=8) -> { shape, rotation }
const LOOKUP = new Map();
for (const shape of SHAPES) for (let r = 3; r >= 0; r--) {
  const p = rotatePattern(CANONICAL[shape], r);
  LOOKUP.set(p[0] | (p[1] << 1) | (p[2] << 2) | (p[3] << 3), { shape, rotation: r });   // r = 0 wins for symmetric shapes
}

/** Shape, rotation and base level of a tile from its four corner levels; throws if the corners span more than one level. */
export function classify(c) {
  const lo = Math.min(c[0], c[1], c[2], c[3]);
  const p = [c[0] - lo, c[1] - lo, c[2] - lo, c[3] - lo];
  if (p.some((v) => v > 1)) throw new Error(`tile corners [${Array.from(c)}] span more than one level`);
  return { ...LOOKUP.get(p[0] | (p[1] << 1) | (p[2] << 2) | (p[3] << 3)), base: lo };
}

/** Bilinear surface of corner heights h [TR, BR, BL, TL] at local (u, v): u west->east, v north->south, both 0..1. */
export function bilinear(h, u, v) {
  const n = h[TL] + (h[TR] - h[TL]) * u, s = h[BL] + (h[BR] - h[BL]) * u;
  return n + (s - n) * v;
}

/** Ground height (metres) at world (x, z) on the tile grid, or null outside. */
export function tileHeightAt(grid, x, z) {
  const t = grid.tileAt(x, z);
  if (!t) return null;
  const c = grid.corners(t.tx, t.tz);
  return bilinear(c, x - t.tx, z - t.tz) * STEP;
}

/** Upward unit normal of the bilinear surface at world (x, z) (for tilting props), or [0, 1, 0] outside. */
export function tileNormalAt(grid, x, z) {
  const t = grid.tileAt(x, z);
  if (!t) return [0, 1, 0];
  const c = grid.corners(t.tx, t.tz), u = x - t.tx, v = z - t.tz;
  const gx = ((c[TR] - c[TL]) * (1 - v) + (c[BR] - c[BL]) * v) * STEP;
  const gz = ((c[BL] - c[TL]) * (1 - u) + (c[BR] - c[TR]) * u) * STEP;
  const l = Math.hypot(gx, 1, gz);
  return [-gx / l, 1 / l, -gz / l];
}

/** Counts of each shape in a grid (for the stats panel and tests). */
export function shapeCounts(grid) {
  const out = Object.fromEntries(SHAPES.map((s) => [s, 0]));
  for (let tz = 0; tz < grid.depth; tz++) for (let tx = 0; tx < grid.width; tx++) out[classify(grid.corners(tx, tz)).shape]++;
  return out;
}
