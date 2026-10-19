// Extra solid shapes for the generators (all flat-shaded, vertex-coloured, y up): cylinders / cones, pyramids, puffy foliage blobs, lying logs.
import { rock, tint, norm, sub, cross, add, scale, lerp3 } from './builder.mjs';

const CONST = () => 0.5;   // a "random" source that always answers 0.5: rock() then gives an exact, un-jittered sphere

/** Upright (truncated) cone / cylinder: radius r0 at the bottom, r1 at the top, `n` sides. */
export function cylinder(prim, cx, y0, cz, r0, r1, h, n, color, { capTop = true, capBottom = false, spin = 0, shade = 0.14 } = {}) {
  const P = (a, y, r) => [cx + Math.sin(a + spin) * r, y, cz + Math.cos(a + spin) * r];
  for (let i = 0; i < n; i++) {
    const a0 = (i / n) * Math.PI * 2, a1 = ((i + 1) / n) * Math.PI * 2, k = 1 - shade / 2 + shade * (0.5 + 0.5 * Math.cos((a0 + a1) / 2 - 2.2));
    prim.quad(P(a0, y0, r0), P(a1, y0, r0), P(a1, y0 + h, r1), P(a0, y0 + h, r1), tint(color, k));
    if (capTop && r1 > 1e-5) prim.tri([cx, y0 + h, cz], P(a0, y0 + h, r1), P(a1, y0 + h, r1), tint(color, 1.12));
    if (capBottom) prim.tri([cx, y0, cz], P(a1, y0, r0), P(a0, y0, r0), tint(color, 0.7));
  }
}

/** A cone is a cylinder that ends in a point. */
export const cone = (prim, cx, y0, cz, r, h, n, color, opts = {}) => cylinder(prim, cx, y0, cz, r, 0, h, n, color, { ...opts, capTop: false });

/** Four-sided pyramid on a rectangle (hip roofs, finials), base centred on (cx, cz). */
export function pyramid(prim, cx, y0, cz, hw, hd, h, color, { tipX = 0, tipZ = 0 } = {}) {
  const A = [cx - hw, y0, cz + hd], B = [cx + hw, y0, cz + hd], C = [cx + hw, y0, cz - hd], D = [cx - hw, y0, cz - hd], T = [cx + tipX, y0 + h, cz + tipZ];
  prim.tri(A, B, T, tint(color, 1.0)); prim.tri(B, C, T, tint(color, 0.88)); prim.tri(C, D, T, tint(color, 0.72)); prim.tri(D, A, T, tint(color, 0.82));
}

/** A puffy foliage blob (icosphere, low poly). `rng` jitters it; pass null for a clean sphere. */
export const blob = (prim, cx, cy, cz, rx, ry, rz, color, rng = null, detail = 1) => rock(prim, cx, cy, cz, rx, ry, rz, rng ?? CONST, color, { detail });

/** A log lying along z (hay bales, fallen trunks): cylinder rotated onto its side, centred on (cx, cy, cz). */
export function lyingCylinder(prim, cx, cy, cz, r, len, n, color, { capColor = null } = {}) {
  const P = (a, z, rad) => [cx + Math.sin(a) * rad, cy + Math.cos(a) * rad, cz + z];
  for (let i = 0; i < n; i++) {
    const a0 = (i / n) * Math.PI * 2, a1 = ((i + 1) / n) * Math.PI * 2;
    const k = 0.9 + 0.2 * Math.cos((a0 + a1) / 2);
    prim.quad(P(a1, -len / 2, r), P(a0, -len / 2, r), P(a0, len / 2, r), P(a1, len / 2, r), tint(color, k));
    prim.tri([cx, cy, cz + len / 2], P(a1, len / 2, r), P(a0, len / 2, r), tint(capColor ?? color, 1.1));
    prim.tri([cx, cy, cz - len / 2], P(a0, -len / 2, r), P(a1, -len / 2, r), tint(capColor ?? color, 0.8));
  }
}

/** Arched door opening outline: a half-ring of wedge blocks round a doorway (stone arch). */
export function archRing(prim, cx, cy, cz, rIn, rOut, depth, n, color, { z0 = 0 } = {}) {
  for (let i = 0; i < n; i++) {
    const a0 = Math.PI * (i / n), a1 = Math.PI * ((i + 1) / n);
    const p = (a, r, z) => [cx + Math.cos(a) * r, cy + Math.sin(a) * r, cz + z];
    const col = tint(color, 0.9 + 0.2 * ((i * 7) % 5) / 5);
    prim.quad(p(a0, rOut, z0 + depth), p(a1, rOut, z0 + depth), p(a1, rIn, z0 + depth), p(a0, rIn, z0 + depth), col);   // front face
    prim.quad(p(a0, rOut, z0), p(a1, rOut, z0), p(a1, rOut, z0 + depth), p(a0, rOut, z0 + depth), tint(col, 1.1));       // outer edge
    prim.quad(p(a1, rIn, z0), p(a0, rIn, z0), p(a0, rIn, z0 + depth), p(a1, rIn, z0 + depth), tint(col, 0.7));            // inner edge
  }
}
