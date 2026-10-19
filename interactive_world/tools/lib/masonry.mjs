// Stone masonry: courses of bevelled blocks in a running bond, with holes for windows and doors.
//
// Local frame of every wall: +x runs along the wall, +y is up, +z points out of the front face. The wall body is behind z = 0.
// `facing` is the world angle of the outward normal: 0 = +z, PI/2 = +x, PI = -z, 3PI/2 = -x.
import { block, slab, tint, T4, RY, hex, mix } from './builder.mjs';

export const STONE = {
  sand: hex('#a39478'),
  tan: hex('#7f7260'),
  brown: hex('#615548'),
  dark: hex('#3d3731'),
  deep: hex('#3a332d'),
  core: hex('#241f1b'),
};

/** Weighted stone colour: mostly tan/brown with sand and dark accents, like the reference's hand-painted blocks. */
export function stoneColor(rng, tone = 'mid') {
  const r = rng();
  const table = {
    light: [[0.28, STONE.sand], [0.42, STONE.tan], [0.25, STONE.brown], [0.05, STONE.dark]],
    mid: [[0.1, STONE.sand], [0.33, STONE.tan], [0.35, STONE.brown], [0.22, STONE.dark]],
    dark: [[0.05, STONE.sand], [0.15, STONE.tan], [0.4, STONE.brown], [0.4, STONE.dark]],
  }[tone];
  let acc = 0;
  for (const [w, c] of table) {
    acc += w;
    if (r <= acc) return tint(c, 0.94 + rng() * 0.12, rng, 0.03);
  }
  return STONE.tan;
}

const overlaps = (h, u0, u1, v0, v1) => !(u1 <= h.u0 || u0 >= h.u1 || v1 <= h.v0 || v0 >= h.v1);

/**
 * One wall. Returns nothing; adds a dark core and the blocks to `prim`.
 * opts: x, z (world position of the wall's start on its front plane), facing, length, y0, y1, thick, rng, tone,
 *       courseH [min,max], blockL [min,max], holes [{u0,u1,v0,v1}], gap, bevel, bond (course offset seed), shadeBottom
 */
export function wall(prim, o) {
  const {
    x, z, facing, length, y0, y1, thick = 0.1, rng, tone = 'mid', courseH = [0.075, 0.115], blockL = [0.13, 0.28],
    holes = [], gap = 0.008, bevel = 0.011, shadeBottom = 0.78, endCaps = false, topFaces = true,
  } = o;
  prim.with(T4(x, 0, z), () => prim.with(RY(facing), () => {
    // dark core: seen through the gaps between blocks
    slab(prim, 0, y0, -thick, length, y1, -0.02, STONE.core, topFaces ? 'st' : 's');
    let y = y0;
    let course = 0;
    while (y < y1 - 0.004) {
      const h = Math.min(courseH[0] + rng() * (courseH[1] - courseH[0]), y1 - y);
      let u = course % 2 ? -rng() * blockL[0] * 0.6 : 0;
      while (u < length) {
        const l = blockL[0] + rng() * (blockL[1] - blockL[0]);
        let a = Math.max(0, u), b = Math.min(length, u + l);
        u += l;
        if (b - a < 0.045) continue;
        const bu0 = a + gap / 2, bu1 = b - gap / 2, bv0 = y + gap / 2, bv1 = y + h - gap / 2;
        if (holes.some((hole) => overlaps(hole, bu0, bu1, bv0, bv1))) continue;
        const k = shadeBottom + (1 - shadeBottom) * Math.min(1, Math.max(0, (y - y0) / 0.7));
        const color = tint(stoneColor(rng, tone), k);
        const out = (rng() - 0.3) * 0.016; // some blocks stand proud, some sink: a hand-built wall
        const faces = 'st' + (a <= 0.001 ? 'w' : '') + (b >= length - 0.001 ? 'e' : '');
        block(prim, bu0, bv0, -thick, bu1, bv1, out, color, { bevel, faces });
      }
      y += h;
      course++;
    }
  }));
}

/**
 * Four walls of an axis-aligned building with corner quoins (alternating long and short tan blocks up each corner).
 * holes: { s: [...], n: [...], e: [...], w: [...] } in each face's own (u, v) coordinates (u runs left to right seen from outside).
 */
export function wallBox(prim, o) {
  const { x0, z0, x1, z1, y0, y1, rng, tone = 'mid', thick = 0.1, holes = {}, quoin = 0.055, sides = 'snew', ...rest } = o;
  const q = quoin;
  const W = x1 - x0, D = z1 - z0;
  const common = { y0, y1, rng, tone, thick, ...rest };
  if (sides.includes('s')) wall(prim, { x: x0 + q, z: z1, facing: 0, length: W - 2 * q, holes: shift(holes.s, -q), ...common });
  if (sides.includes('e')) wall(prim, { x: x1, z: z1 - q, facing: Math.PI / 2, length: D - 2 * q, holes: shift(holes.e, -q), ...common });
  if (sides.includes('n')) wall(prim, { x: x1 - q, z: z0, facing: Math.PI, length: W - 2 * q, holes: shift(holes.n, -q), ...common });
  if (sides.includes('w')) wall(prim, { x: x0, z: z0 + q, facing: (3 * Math.PI) / 2, length: D - 2 * q, holes: shift(holes.w, -q), ...common });
  // quoins
  for (const [cx, cz] of [[x0, z0], [x1, z0], [x0, z1], [x1, z1]]) {
    const sx = cx === x0 ? 1 : -1, sz = cz === z0 ? 1 : -1;
    let y = y0, i = 0;
    while (y < y1 - 0.01) {
      const h = Math.min(0.1 + rng() * 0.05, y1 - y);
      const long = i % 2 === 0;
      const ax = long ? q * 1.9 : q * 1.0, az = long ? q * 1.0 : q * 1.9;
      const color = tint(stoneColor(rng, 'light'), 0.95 + rng() * 0.1);
      const bx0 = Math.min(cx, cx + sx * ax) - (sx > 0 ? 0.008 : 0), bx1 = Math.max(cx, cx + sx * ax) + (sx > 0 ? 0 : 0.008);
      const bz0 = Math.min(cz, cz + sz * az) - (sz > 0 ? 0.008 : 0), bz1 = Math.max(cz, cz + sz * az) + (sz > 0 ? 0 : 0.008);
      block(prim, bx0, y + 0.004, bz0, bx1, y + h - 0.004, bz1, color, { bevel: 0.012, faces: 'nsewt' });
      y += h;
      i++;
    }
  }
}

const shift = (holes = [], du) => holes.map((h) => ({ ...h, u0: h.u0 + du, u1: h.u1 + du }));

/**
 * A round tower body: rings of blocks around a vertical axis. `r(y)` may taper. Holes are by angle (radians, 0 = +z) and height.
 * holes: [{a0, a1, v0, v1}] with a0 < a1 (angles in radians, any range; compared modulo 2PI)
 */
export function roundTower(prim, o) {
  const { cx, cz, r, y0, y1, rng, tone = 'mid', thick = 0.12, courseH = [0.075, 0.12], blockL = [0.14, 0.26], holes = [], gap = 0.008, bevel = 0.011, taper = 0 } = o;
  const radiusAt = (y) => r * (1 - taper * ((y - y0) / (y1 - y0)));
  // dark core cylinder
  const N = 24;
  for (let i = 0; i < N; i++) {
    const a0 = (i / N) * Math.PI * 2, a1 = ((i + 1) / N) * Math.PI * 2;
    const rc = r - 0.02;
    const P = (a, y, rad) => [cx + Math.sin(a) * rad, y, cz + Math.cos(a) * rad];
    prim.quad(P(a0, y0, rc), P(a1, y0, rc), P(a1, y1, rc * (1 - taper)), P(a0, y1, rc * (1 - taper)), STONE.core);
  }
  let y = y0, course = 0;
  const twoPi = Math.PI * 2;
  const inHole = (a, y0b, y1b, wA) => holes.some((h) => {
    const mid = (h.a0 + h.a1) / 2, half = (h.a1 - h.a0) / 2 + wA;
    let d = Math.abs((((a - mid) % twoPi) + 3 * Math.PI) % twoPi - Math.PI);
    return d <= half && !(y1b <= h.v0 || y0b >= h.v1);
  });
  while (y < y1 - 0.004) {
    const h = Math.min(courseH[0] + rng() * (courseH[1] - courseH[0]), y1 - y);
    const rad = radiusAt(y + h / 2);
    const circumference = twoPi * rad;
    let a = course % 2 ? (rng() * blockL[0] * 0.5) / rad : 0;
    const start = a;
    while (a < twoPi + start - 0.01) {
      const l = (blockL[0] + rng() * (blockL[1] - blockL[0])) / rad;
      const a0 = a, a1 = Math.min(a + l, twoPi + start);
      a += l;
      const mid = (a0 + a1) / 2;
      const chord = 2 * rad * Math.sin((a1 - a0) / 2) - gap;
      if (chord < 0.04) continue;
      if (inHole(mid, y, y + h, ((a1 - a0) / 2) * 0.35)) continue;
      const k = 0.8 + 0.2 * Math.min(1, Math.max(0, (y - y0) / 0.8));
      const color = tint(stoneColor(rng, tone), k);
      const out = (rng() - 0.3) * 0.016;
      prim.with(T4(cx + Math.sin(mid) * rad, 0, cz + Math.cos(mid) * rad), () => prim.with(RY(mid), () => {
        block(prim, -chord / 2, y + gap / 2, -thick, chord / 2, y + h - gap / 2, out, color, { bevel, faces: 'stew' });
      }));
    }
    y += h;
    course++;
  }
}
