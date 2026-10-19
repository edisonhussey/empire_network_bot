// The one triangulation rule, shared by the mesh builder and every height query, so what is drawn and what is asked always agree.
//
// Each cell's quad is split along the TL-BR diagonal into two triangles, always the same way:
//   A = (TL, BL, BR)   lower-left half  (local v >= u)
//   B = (TL, BR, TR)   upper-right half (local v <  u)
// Both are wound counter-clockwise seen from above (+Y), so their normals point up.
//
//   TL ------- TR        u = (x - x0) / cellSize  (0 at the west edge, 1 at the east edge)
//    |  \   B  |         v = (z - z0) / cellSize  (0 at the north edge, 1 at the south edge)
//    |   \     |
//    | A   \   |
//   BL ------- BR
import { TR, BR, BL, TL } from './grid.js';

export const TRIANGLES = [[TL, BL, BR], [TL, BR, TR]];

/** Height of the cell surface at local (u, v) in [0, 1]^2, exactly on the drawn triangles. */
export function surfaceHeightLocal(s, u, v) {
  if (v >= u) return s[TL] + (s[BL] - s[TL]) * v + (s[BR] - s[BL]) * u;   // triangle A
  return s[TL] + (s[TR] - s[TL]) * u + (s[BR] - s[TR]) * v;               // triangle B
}

/** Ground height at world (x, z): the top surface of the cell that contains it (null outside the map). Assets will stand on this. */
export function heightAt(grid, x, z) {
  const c = grid.cellAt(x, z);
  if (!c) return null;
  const u = x / grid.cellSize - c.cx, v = z / grid.cellSize - c.cz;
  return surfaceHeightLocal(grid.surface(c.cx, c.cz), u, v);
}

/** Upward unit normal of the triangle under local (u, v); the slope of the ground there is acos(n[1]). */
export function surfaceNormalLocal(s, u, v, cellSize) {
  // gradient of the plane of the active triangle: dh/dx, dh/dz
  const gx = v >= u ? (s[BR] - s[BL]) / cellSize : (s[TR] - s[TL]) / cellSize;
  const gz = v >= u ? (s[BL] - s[TL]) / cellSize : (s[BR] - s[TR]) / cellSize;
  const l = Math.hypot(gx, 1, gz);
  return [-gx / l, 1 / l, -gz / l];
}
