// Layer 2, pass 2: reconstruction. Neighbouring cells are independent, so the edge they share can sit at two different heights. This pass walks
// every cell's EAST and SOUTH edge (so each interior edge is visited exactly once), compares the two cells' heights at both ends of that edge, and
// fills the vertical gap between the two edge lines with wall faces ("connectors"): cliffs where the jump is big, small risers where it is tiny.
//
// The shared edge P -> Q is a vertical plane. Side 1 is this cell's edge line (h1P -> h1Q), side 2 the neighbour's (h2P -> h2Q). With
// dP = h1P - h2P and dQ = h1Q - h2Q:
//   * both ~0                -> the edges coincide, nothing to fill;
//   * same sign (or one ~0)  -> one quad (or triangle) between the two lines;
//   * opposite signs         -> the lines cross: two triangles meeting at the crossing point.
// Each face's normal is horizontal and points towards the lower side, so a wall is lit and seen from the side it faces.
// The map border gets the same treatment against a flat base level ("skirt"), so the world reads as a solid slab.
import { TR, BR, BL, TL } from './grid.js';

export const EPS = 0.005;   // metres: edge mismatches smaller than this are treated as joined (5 mm, invisible)

/** Wall triangle with an explicit horizontal normal (nx, 0, nz); the winding is flipped if needed so it faces that way. */
function pushWall(pos, nor, a, b, c, nx, nz) {
  const ux = b[0] - a[0], uy = b[1] - a[1], uz = b[2] - a[2], vx = c[0] - a[0], vy = c[1] - a[1], vz = c[2] - a[2];
  const fx = uy * vz - uz * vy, fz = ux * vy - uy * vx;
  if (fx * nx + fz * nz < 0) [b, c] = [c, b];
  pos.push(a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]);
  nor.push(nx, 0, nz, nx, 0, nz, nx, 0, nz);
}

/**
 * Fill between two edge lines over the same ground segment P -> Q (P, Q = [x, z]). Side 1 heights h1P, h1Q; side 2 h2P, h2Q.
 * (ox, oz) is the horizontal direction from side 1 towards side 2. Returns the number of triangles added.
 */
export function fillEdgeGap(pos, nor, P, Q, h1P, h1Q, h2P, h2Q, ox, oz) {
  const dP = h1P - h2P, dQ = h1Q - h2Q;
  const P1 = [P[0], h1P, P[1]], Q1 = [Q[0], h1Q, Q[1]], P2 = [P[0], h2P, P[1]], Q2 = [Q[0], h2Q, Q[1]];
  const face = (d) => (d > 0 ? [ox, oz] : [-ox, -oz]);   // towards the lower side
  const zP = Math.abs(dP) < EPS, zQ = Math.abs(dQ) < EPS;
  if (zP && zQ) return 0;
  if (zP || zQ || Math.sign(dP) === Math.sign(dQ)) {
    const [nx, nz] = face(zP ? dQ : dP);
    let n = 0;
    if (!zP) { pushWall(pos, nor, P1, Q1, P2, nx, nz); n++; }
    if (!zQ) { pushWall(pos, nor, Q1, Q2, P2, nx, nz); n++; }
    return n;
  }
  // the lines cross at t: one triangle on each side of the crossing, facing opposite ways
  const t = dP / (dP - dQ);
  const X = [P[0] + (Q[0] - P[0]) * t, h1P + (h1Q - h1P) * t, P[1] + (Q[1] - P[1]) * t];
  let [nx, nz] = face(dP);
  pushWall(pos, nor, P1, X, P2, nx, nz);
  [nx, nz] = face(dQ);
  pushWall(pos, nor, X, Q1, Q2, nx, nz);
  return 2;
}

/** Connector walls owned by the cells in `range` (their east and south edges): { positions, normals, triangles, edges }. */
export function buildConnectors(grid, range) {
  const pos = [], nor = [], s = grid.cellSize;
  let tris = 0, edges = 0;
  for (let cz = range.z0; cz < range.z1; cz++) {
    for (let cx = range.x0; cx < range.x1; cx++) {
      const h = grid.surface(cx, cz);
      const x1 = (cx + 1) * s, z0 = cz * s, z1 = (cz + 1) * s, x0 = cx * s;
      if (cx + 1 < grid.width) {   // east edge: my TR-BR against the east neighbour's TL-BL, at x = x1
        const e = grid.surface(cx + 1, cz);
        const n = fillEdgeGap(pos, nor, [x1, z0], [x1, z1], h[TR], h[BR], e[TL], e[BL], 1, 0);
        tris += n; edges += n > 0;
      }
      if (cz + 1 < grid.depth) {   // south edge: my BL-BR against the south neighbour's TL-TR, at z = z1
        const so = grid.surface(cx, cz + 1);
        const n = fillEdgeGap(pos, nor, [x0, z1], [x1, z1], h[BL], h[BR], so[TL], so[TR], 0, 1);
        tris += n; edges += n > 0;
      }
    }
  }
  return { positions: new Float32Array(pos), normals: new Float32Array(nor), triangles: tris, edges };
}

/** Walls from the map border down to `base` (metres) for border cells in `range`, facing outwards: the side of the terrain slab. */
export function buildSkirt(grid, range, base) {
  const pos = [], nor = [], s = grid.cellSize;
  let tris = 0;
  const wall = (P, Q, hP, hQ, ox, oz) => { tris += fillEdgeGap(pos, nor, P, Q, base, base, hP, hQ, -ox, -oz); };   // the outside is side 1 at `base`
  for (let cz = range.z0; cz < range.z1; cz++) {
    for (let cx = range.x0; cx < range.x1; cx++) {
      const h = grid.surface(cx, cz), x0 = cx * s, x1 = x0 + s, z0 = cz * s, z1 = z0 + s;
      if (cx === 0) wall([x0, z0], [x0, z1], h[TL], h[BL], -1, 0);
      if (cx === grid.width - 1) wall([x1, z0], [x1, z1], h[TR], h[BR], 1, 0);
      if (cz === 0) wall([x0, z0], [x1, z0], h[TL], h[TR], 0, -1);
      if (cz === grid.depth - 1) wall([x0, z1], [x1, z1], h[BL], h[BR], 0, 1);
    }
  }
  return { positions: new Float32Array(pos), normals: new Float32Array(nor), triangles: tris };
}
