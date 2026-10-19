// Layer 2, building placement on the cell grid (pure data, no three.js). Rules: architecture/ASSET_GLB_PROTOCOL.md.
//
// A building GLB is authored facing NORTH (-Z), with its origin at the centre of its footprint on the ground, and a footprint of
// whole cells (w x d cells along x x z when it faces north). Placing it means: pick the cell rect it covers, turn it to face
// N / E / S / W, and lift it to the ground height. Turning by 90 or 270 degrees swaps the rect's width and depth.

/** Facing -> rotation about +Y (radians, counter-clockwise seen from above). A north-facing asset turned by this faces that way. */
export const FACING_ROTATION = { N: 0, W: Math.PI / 2, S: Math.PI, E: -Math.PI / 2 };

/** Unit vector (x, z) each facing points to. */
export const FACING_DIR = { N: [0, -1], E: [1, 0], S: [0, 1], W: [-1, 0] };

/**
 * Where a building goes.
 * @param {{ cx: number, cz: number, w: number, d: number, facing?: 'N'|'E'|'S'|'W', cellSize?: number }} p
 *   cx, cz: the north-west (min x, min z) cell of the covered rect; w, d: the asset's footprint in cells when facing north.
 * @returns {{ rect: { x0, z0, x1, z1 }, x: number, z: number, rotationY: number, facing: string }}
 *   rect in cells (x1, z1 exclusive); x, z = world position of the asset origin (rect centre).
 */
export function placeOnCells({ cx, cz, w, d, facing = 'N', cellSize = 5 }) {
  if (!(facing in FACING_ROTATION)) throw new Error(`facing must be N, E, S or W, got ${facing}`);
  const turned = facing === 'E' || facing === 'W';
  const rw = turned ? d : w, rd = turned ? w : d;
  const rect = { x0: cx, z0: cz, x1: cx + rw, z1: cz + rd };
  return { rect, x: (cx + rw / 2) * cellSize, z: (cz + rd / 2) * cellSize, rotationY: FACING_ROTATION[facing], facing };
}

/** Turn an asset-local point (x, z) of a north-facing asset into its world offset for `facing` (same maths three.js uses). */
export function rotateLocal(x, z, facing) {
  const a = FACING_ROTATION[facing], c = Math.cos(a), s = Math.sin(a);
  return [c * x + s * z, -s * x + c * z];
}

/**
 * Is the rect buildable as it stands: every corner of every covered cell at one height (within `tolerance` metres)?
 * @returns {{ ok: boolean, level: number, spread: number }} level = the lowest corner (where the asset's y = 0 goes).
 */
export function footprintLevel(grid, rect, tolerance = 0.001) {
  let lo = Infinity, hi = -Infinity;
  for (let cz = rect.z0; cz < rect.z1; cz++) for (let cx = rect.x0; cx < rect.x1; cx++) {
    if (!grid.inBounds(cx, cz)) return { ok: false, level: NaN, spread: Infinity };
    for (const h of grid.surface(cx, cz)) { if (h < lo) lo = h; if (h > hi) hi = h; }
  }
  return { ok: hi - lo <= tolerance, level: lo, spread: hi - lo };
}
