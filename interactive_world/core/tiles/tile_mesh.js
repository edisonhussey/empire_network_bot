// Layer 2 for tiles: chunk meshes as plain typed arrays (no three.js), built from the five tile templates.
//
// buildTileTops: every tile is its shape's template (the kit GLB's Tile_* mesh, or templateFromFormula in node) turned by its rotation and
// lifted to its base level, then all copies in the range are merged and given SMOOTH normals (vertices at the same point are welded for
// shading), which is the guide's "same data, smooth result": the logical surface stays exactly the quantized corners, only the shading blends.
// Where neighbours disagree the vertices do not coincide, so cliffs and risers keep a crisp edge.
//
// buildTileWalls: the reconstruction pass of TERRAIN_CELL_ARCHITECTURE section 4 (fillEdgeGap), on tile heights, plus the island skirt.
// Each wall vertex carries `soil` (1 for risers <= 0.4 m and the skirt, 0 for rock cliffs) for the wall shader.
import { STEP, TR, BR, BL, TL } from './tile_grid.js';
import { classify, SUBDIV, CANONICAL, bilinear } from './tile_shapes.js';
import { fillEdgeGap } from '../terrain/mesh_connectors.js';
import { RISER_MAX } from './tile_edges.js';

/** A tile template from the formula: non-indexed triangles in the three.js frame (x east, y up, z south), x, z in [-0.5, 0.5]. */
export function templateFromFormula(shape) {
  const c = CANONICAL[shape], N = SUBDIV, out = [];
  const P = (i, j) => { const u = i / N, v = j / N; return [u - 0.5, bilinear(c, u, v) * STEP, v - 0.5]; };   // j counts southwards
  for (let i = 0; i < N; i++) for (let j = 0; j < N; j++) {
    const nw = P(i, j), ne = P(i + 1, j), se = P(i + 1, j + 1), sw = P(i, j + 1);
    out.push(...nw, ...sw, ...se, ...nw, ...se, ...ne);   // CCW seen from above, split along TL-BR
  }
  return new Float32Array(out);
}

/** Rotate a template point by r quarter turns about +Y (three.js rotation.y = r * PI / 2). */
function turn(x, z, r) {
  switch (r & 3) {
    case 0: return [x, z];
    case 1: return [z, -x];
    case 2: return [-x, -z];
    default: return [-z, x];
  }
}

/**
 * Merged top surface for tiles in `range` ({ x0, z0, x1, z1 }, end exclusive).
 * @param templates { [shape]: Float32Array } non-indexed triangle positions per shape
 * @returns {{ positions: Float32Array, normals: Float32Array, tileOfTri: Uint32Array, triangles: number }}
 */
export function buildTileTops(grid, templates, range) {
  let count = 0;
  for (let tz = range.z0; tz < range.z1; tz++) for (let tx = range.x0; tx < range.x1; tx++) count += templates[classify(grid.corners(tx, tz)).shape].length;
  const pos = new Float32Array(count), nor = new Float32Array(count), tileOfTri = new Uint32Array(count / 9);
  let o = 0;
  for (let tz = range.z0; tz < range.z1; tz++) for (let tx = range.x0; tx < range.x1; tx++) {
    const { shape, rotation, base } = classify(grid.corners(tx, tz));
    const t = templates[shape], y0 = base * STEP;
    const first = o / 9;
    for (let k = 0; k < t.length; k += 3) {
      const [x, z] = turn(t[k], t[k + 2], rotation);
      pos[o++] = tx + 0.5 + x; pos[o++] = y0 + t[k + 1]; pos[o++] = tz + 0.5 + z;
    }
    tileOfTri.fill(grid.index(tx, tz), first, o / 9);
  }
  // smooth normals: area-weighted face normals accumulated on welded points
  const acc = new Map(), key = (i) => `${Math.round(pos[i] * 1000)},${Math.round(pos[i + 1] * 1000)},${Math.round(pos[i + 2] * 1000)}`;
  const face = new Float32Array(3);
  for (let i = 0; i < pos.length; i += 9) {
    const ux = pos[i + 3] - pos[i], uy = pos[i + 4] - pos[i + 1], uz = pos[i + 5] - pos[i + 2];
    const vx = pos[i + 6] - pos[i], vy = pos[i + 7] - pos[i + 1], vz = pos[i + 8] - pos[i + 2];
    face[0] = uy * vz - uz * vy; face[1] = uz * vx - ux * vz; face[2] = ux * vy - uy * vx;
    if (face[1] < 0) { face[0] = -face[0]; face[1] = -face[1]; face[2] = -face[2]; }   // tops always face up
    for (let k = 0; k < 9; k += 3) {
      const kk = key(i + k);
      const a = acc.get(kk);
      if (a) { a[0] += face[0]; a[1] += face[1]; a[2] += face[2]; } else acc.set(kk, [face[0], face[1], face[2]]);
    }
  }
  for (let i = 0; i < pos.length; i += 3) {
    const a = acc.get(key(i)), l = Math.hypot(a[0], a[1], a[2]) || 1;
    nor[i] = a[0] / l; nor[i + 1] = a[1] / l; nor[i + 2] = a[2] / l;
  }
  return { positions: pos, normals: nor, tileOfTri, triangles: pos.length / 9 };
}

/** Walls between disagreeing tiles in `range` (east and south edges), plus the skirt down to `skirtBase` on border tiles. */
export function buildTileWalls(grid, range, { skirtBase = -3 } = {}) {
  const pos = [], nor = [], soil = [];
  const h = (tx, tz, k) => grid.cornerHeight(tx, tz, k);
  const add = (args, isSoil) => {
    const n = fillEdgeGap(pos, nor, ...args);
    for (let k = 0; k < n * 3; k++) soil.push(isSoil);
  };
  const stepOf = (a, b, c, d) => Math.max(Math.abs(a - c), Math.abs(b - d));
  for (let tz = range.z0; tz < range.z1; tz++) for (let tx = range.x0; tx < range.x1; tx++) {
    const x0 = tx, x1 = tx + 1, z0 = tz, z1 = tz + 1;
    if (tx + 1 < grid.width) {
      const a = [h(tx, tz, TR), h(tx, tz, BR), h(tx + 1, tz, TL), h(tx + 1, tz, BL)];
      add([[x1, z0], [x1, z1], ...a, 1, 0], stepOf(...a) <= RISER_MAX ? 1 : 0);
    }
    if (tz + 1 < grid.depth) {
      const a = [h(tx, tz, BL), h(tx, tz, BR), h(tx, tz + 1, TL), h(tx, tz + 1, TR)];
      add([[x0, z1], [x1, z1], ...a, 0, 1], stepOf(...a) <= RISER_MAX ? 1 : 0);
    }
    const B = skirtBase;
    if (tx === 0) add([[x0, z0], [x0, z1], B, B, h(tx, tz, TL), h(tx, tz, BL), 1, 0], 1);
    if (tx === grid.width - 1) add([[x1, z0], [x1, z1], B, B, h(tx, tz, TR), h(tx, tz, BR), -1, 0], 1);
    if (tz === 0) add([[x0, z0], [x1, z0], B, B, h(tx, tz, TL), h(tx, tz, TR), 0, 1], 1);
    if (tz === grid.depth - 1) add([[x0, z1], [x1, z1], B, B, h(tx, tz, BL), h(tx, tz, BR), 0, -1], 1);
  }
  return { positions: new Float32Array(pos), normals: new Float32Array(nor), soil: new Float32Array(soil), triangles: pos.length / 9 };
}

/** Water surfaces: one quad per water tile at its water level (non-indexed), and where water reaches the map border a side face down to the bed (drawn double-sided). */
export function buildWater(grid, range) {
  const pos = [];
  for (let tz = range.z0; tz < range.z1; tz++) for (let tx = range.x0; tx < range.x1; tx++) {
    const y = grid.waterHeight(tx, tz);
    if (y === null) continue;
    pos.push(tx, y, tz, tx, y, tz + 1, tx + 1, y, tz + 1, tx, y, tz, tx + 1, y, tz + 1, tx + 1, y, tz);
    const side = (ax, az, bx, bz, ha, hb) => pos.push(ax, y, az, bx, hb, bz, bx, y, bz, ax, y, az, ax, ha, az, bx, hb, bz);   // outward-facing
    if (tx === 0) side(tx, tz + 1, tx, tz, grid.cornerHeight(tx, tz, BL), grid.cornerHeight(tx, tz, TL));
    if (tx === grid.width - 1) side(tx + 1, tz, tx + 1, tz + 1, grid.cornerHeight(tx, tz, TR), grid.cornerHeight(tx, tz, BR));
    if (tz === 0) side(tx, tz, tx + 1, tz, grid.cornerHeight(tx, tz, TL), grid.cornerHeight(tx, tz, TR));
    if (tz === grid.depth - 1) side(tx + 1, tz + 1, tx, tz + 1, grid.cornerHeight(tx, tz, BR), grid.cornerHeight(tx, tz, BL));
  }
  return { positions: new Float32Array(pos), triangles: pos.length / 9 };
}
