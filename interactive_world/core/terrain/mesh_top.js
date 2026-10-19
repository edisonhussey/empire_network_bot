// Layer 2, pass 1: top surfaces. Every cell becomes two triangles straight from its own four corner heights (triangulation: surface.js).
// Output is plain typed arrays (no three.js), non-indexed and flat-shaded, so each cell reads as its own plane and the builder runs anywhere
// (main thread, worker, node tests). A "range" is a block of cells { x0, z0, x1, z1 } (x1 / z1 exclusive), normally one chunk.
import { CORNER_OFFSET } from './grid.js';
import { TRIANGLES } from './surface.js';

/** Append triangle (a, b, c) with its face normal. Points are [x, y, z]. */
export function pushTriangle(pos, nor, a, b, c) {
  const ux = b[0] - a[0], uy = b[1] - a[1], uz = b[2] - a[2], vx = c[0] - a[0], vy = c[1] - a[1], vz = c[2] - a[2];
  let nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx;
  const l = Math.hypot(nx, ny, nz) || 1;
  nx /= l; ny /= l; nz /= l;
  pos.push(a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]);
  nor.push(nx, ny, nz, nx, ny, nz, nx, ny, nz);
}

/** Top surfaces of all cells in `range`: { positions, normals, triangles }. */
export function buildTopSurfaces(grid, range) {
  const pos = [], nor = [], s = grid.cellSize;
  for (let cz = range.z0; cz < range.z1; cz++) {
    for (let cx = range.x0; cx < range.x1; cx++) {
      const h = grid.surface(cx, cz);
      const p = CORNER_OFFSET.map(([dx, dz], k) => [(cx + dx) * s, h[k], (cz + dz) * s]);
      for (const [a, b, c] of TRIANGLES) pushTriangle(pos, nor, p[a], p[b], p[c]);
    }
  }
  return { positions: new Float32Array(pos), normals: new Float32Array(nor), triangles: pos.length / 9 };
}

/** The outline of every cell's own top quad, lifted `lift` metres (line segments: pairs of points). Shows that cells are independent. */
export function buildCellOutlines(grid, range, lift = 0.03) {
  const out = [], s = grid.cellSize;
  for (let cz = range.z0; cz < range.z1; cz++) {
    for (let cx = range.x0; cx < range.x1; cx++) {
      const h = grid.surface(cx, cz);
      for (let k = 0; k < 4; k++) {
        const a = k, b = (k + 1) % 4;   // TR-BR, BR-BL, BL-TL, TL-TR
        out.push((cx + CORNER_OFFSET[a][0]) * s, h[a] + lift, (cz + CORNER_OFFSET[a][1]) * s, (cx + CORNER_OFFSET[b][0]) * s, h[b] + lift, (cz + CORNER_OFFSET[b][1]) * s);
      }
    }
  }
  return new Float32Array(out);
}
