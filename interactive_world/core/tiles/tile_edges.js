// Edge classification from data alone (no meshes): every tile edge whose two sides disagree, with the side it faces and how tall it is.
// Each tile reports its EAST and SOUTH edge (so every interior edge once), the map border is reported against the skirt base.
//
//   kind 'riser'  max step <= RISER_MAX (0.4 m = 2 levels): walkable, drawn as an earth bank
//   kind 'cliff'  anything taller: blocks walking, drawn with rock-face modules
//   kind 'border' the slab side at the map edge
import { TR, BR, BL, TL, STEP } from './tile_grid.js';

export const RISER_MAX = 0.4 + 1e-6;

/**
 * @returns {Array<{ x, z, ax, az, bx, bz, nx, nz, low, high, step, kind, lowTile, highTile }>}
 *   (ax, az) -> (bx, bz) the edge on the ground, (x, z) its middle, (nx, nz) the horizontal direction the face looks (towards the low side),
 *   low / high = lowest point at the foot and highest point at the top (metres), step = the largest height difference along it.
 */
export function classifyEdges(grid, { border = true, skirtBase = null } = {}) {
  const out = [];
  const h = (tx, tz, k) => grid.cornerHeight(tx, tz, k);
  const add = (ax, az, bx, bz, h1a, h1b, h2a, h2b, ox, oz, t1, t2) => {
    const da = h1a - h2a, db = h1b - h2b;
    if (Math.abs(da) < 1e-6 && Math.abs(db) < 1e-6) return;
    const side1High = da + db > 0;   // which side is on top (by the mean difference; crossing edges are tiny and rare)
    const [hiA, hiB, loA, loB] = side1High ? [h1a, h1b, h2a, h2b] : [h2a, h2b, h1a, h1b];
    const step = Math.max(Math.abs(da), Math.abs(db));
    out.push({
      x: (ax + bx) / 2, z: (az + bz) / 2, ax, az, bx, bz,
      nx: side1High ? ox : -ox, nz: side1High ? oz : -oz,
      low: Math.min(loA, loB), high: Math.max(hiA, hiB), step,
      kind: t2 === null ? 'border' : step <= RISER_MAX ? 'riser' : 'cliff',
      highTile: side1High ? t1 : t2, lowTile: side1High ? t2 : t1,
    });
  };
  for (let tz = 0; tz < grid.depth; tz++) for (let tx = 0; tx < grid.width; tx++) {
    const me = { tx, tz };
    if (tx + 1 < grid.width) add(tx + 1, tz, tx + 1, tz + 1, h(tx, tz, TR), h(tx, tz, BR), h(tx + 1, tz, TL), h(tx + 1, tz, BL), 1, 0, me, { tx: tx + 1, tz });
    if (tz + 1 < grid.depth) add(tx, tz + 1, tx + 1, tz + 1, h(tx, tz, BL), h(tx, tz, BR), h(tx, tz + 1, TL), h(tx, tz + 1, TR), 0, 1, me, { tx, tz: tz + 1 });
    if (!border) continue;
    const b = skirtBase ?? -3;
    if (tx === 0) add(tx, tz, tx, tz + 1, h(tx, tz, TL), h(tx, tz, BL), b, b, -1, 0, me, null);
    if (tx === grid.width - 1) add(tx + 1, tz, tx + 1, tz + 1, h(tx, tz, TR), h(tx, tz, BR), b, b, 1, 0, me, null);
    if (tz === 0) add(tx, tz, tx + 1, tz, h(tx, tz, TL), h(tx, tz, TR), b, b, 0, -1, me, null);
    if (tz === grid.depth - 1) add(tx, tz + 1, tx + 1, tz + 1, h(tx, tz, BL), h(tx, tz, BR), b, b, 0, 1, me, null);
  }
  return out;
}

/** Edge count by kind. */
export function edgeStats(edges) {
  const s = { riser: 0, cliff: 0, border: 0, tallest: 0 };
  for (const e of edges) { s[e.kind]++; if (e.kind === 'cliff') s.tallest = Math.max(s.tallest, e.step); }
  return s;
}

export { STEP };
