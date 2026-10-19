// The stone masses: foundation, hall, keep, terrace walls, crenellations. Windows and doors are holes cut in the masonry here (the
// OPENINGS lists below are the one source of truth) and filled in by details.mjs.
import { block, slab, tint } from '../lib/builder.mjs';
import { wallBox, stoneColor, STONE } from '../lib/masonry.mjs';
import { H, HALL, KEEP, WEST, FRONT, COURT, DOOR_X } from './layout.mjs';

const win = (c, y0 = 0.68, y1 = 0.95, w = 0.11) => ({ c, w, y0, y1, kind: 'window' });

// Each opening: c = centre along the wall (world x for south/north faces, world z for west/east faces), w, y0, y1 absolute heights.
export const OPENINGS = {
  hallS: [win(0.62), win(0.84), win(1.06), { c: 1.34, w: 0.22, y0: H.floor, y1: 0.98, kind: 'door' }, win(1.58)],
  hallN: [win(0.25, 0.72, 0.98), win(0.5, 0.72, 0.98), win(1.6, 0.72, 0.98), win(1.84, 0.72, 0.98), win(2.06, 0.72, 0.98)],
  hallW: [win(0.34), win(0.58)],
  keepS: [win(0.99, 1.55, 1.7, 0.1)],
  keepE: [win(0.26, 1.2, 1.5, 0.1), win(0.26, 1.55, 1.7, 0.1)],
  gate: { c: DOOR_X, w: 0.3, y0: 0.0, y1: 0.46 },
};
export const KEEP_WALL_TOP = H.keepTop - 0.2;

/** Hole list in a wall face's own coordinates (u from the face's left corner, seen from outside). `from` is that corner's world coordinate. */
const faceHoles = (list, from, flip = false) => list.map((o) => {
  const a = o.c - o.w / 2, b = o.c + o.w / 2;
  return flip ? { u0: from - b, u1: from - a, v0: o.y0, v1: o.y1 } : { u0: a - from, u1: b - from, v0: o.y0, v1: o.y1 };
});

/** A row of merlons with a cap band underneath, along a wall top. axis 'x': runs along x at z = fixed; axis 'z': along z at x = fixed. */
export function merlons(ctx, { axis, fixed, from, to, y, depth = 0.1, length = 0.15, gap = 0.1, h = 0.15, cap = 0.075, tone = 'light', skip = () => false }) {
  const { prims: { stone: s }, rng } = ctx;
  const a = Math.min(from, to), b = Math.max(from, to);
  const put = (u0, u1, y0, y1, bevel) => {
    const [x0, x1, z0, z1] = axis === 'x' ? [u0, u1, fixed - depth / 2, fixed + depth / 2] : [fixed - depth / 2, fixed + depth / 2, u0, u1];
    block(s, x0, y0, z0, x1, y1, z1, tint(stoneColor(rng, tone), 1.04), { bevel, faces: 'nsewt' });
  };
  put(a, b, y, y + cap, 0.014);
  const n = Math.max(1, Math.floor((b - a + gap) / (length + gap)));
  const pitch = (b - a - length) / Math.max(1, n - 1);
  for (let i = 0; i < n; i++) {
    const u = n === 1 ? a + (b - a - length) / 2 : a + i * pitch;
    if (!skip(u, u + length)) put(u, u + length, y + cap, y + cap + h, 0.018);
  }
}

export function buildStone(ctx) {
  const { prims, rng } = ctx;
  const s = prims.stone;

  // ---- hall: dark foundation, then the two-storey wall with its windows --------------------------------------------------------------------
  wallBox(s, { ...HALL, y0: 0, y1: H.floor, rng, tone: 'dark', thick: 0.12, courseH: [0.085, 0.12], blockL: [0.16, 0.34] });
  wallBox(s, {
    ...HALL, y0: H.floor, y1: H.eave, rng, tone: 'mid', thick: 0.1,
    holes: {
      s: faceHoles(OPENINGS.hallS, HALL.x0),
      n: faceHoles(OPENINGS.hallN, HALL.x1, true),
      w: faceHoles(OPENINGS.hallW, HALL.z0),
    },
  });

  // ---- keep: tall square tower behind the hall -----------------------------------------------------------------------------------------------
  wallBox(s, {
    ...KEEP, y0: 0, y1: KEEP_WALL_TOP, rng, tone: 'mid', thick: 0.12, courseH: [0.08, 0.125], blockL: [0.15, 0.3],
    holes: { s: faceHoles(OPENINGS.keepS, KEEP.x0), e: faceHoles(OPENINGS.keepE, KEEP.z1, true) },
  });

  // ---- west rampart (the long left wall) and the front terrace wall ----------------------------------------------------------------------
  const wallTop = H.parapet - 0.08;
  wallBox(s, { ...WEST, y0: 0, y1: H.floor, rng, tone: 'dark', thick: 0.13, sides: 'sew', courseH: [0.09, 0.13], blockL: [0.17, 0.36] });
  wallBox(s, { ...WEST, y0: H.floor, y1: wallTop, rng, tone: 'mid', thick: 0.12, sides: 'sew', courseH: [0.08, 0.12] });
  wallBox(s, {
    ...FRONT, y0: 0, y1: H.floor, rng, tone: 'dark', thick: 0.14, sides: 'sne', courseH: [0.09, 0.13], blockL: [0.17, 0.36],
    holes: { s: faceHoles([OPENINGS.gate], FRONT.x0) },
  });
  wallBox(s, { ...FRONT, y0: H.floor, y1: wallTop, rng, tone: 'mid', thick: 0.12, sides: 'sne', courseH: [0.08, 0.12] });

  // ---- crenellations on the outer edges, and the walkway slabs between the parapets ----------------------------------------------------------
  merlons(ctx, { axis: 'z', fixed: 0.06, from: WEST.z0 + 0.12, to: WEST.z1 - 0.05, y: wallTop });
  merlons(ctx, { axis: 'x', fixed: WEST.z1 - 0.06, from: WEST.x0 + 0.1, to: FRONT.x1 - 0.1, y: wallTop });
  slab(s, WEST.x0 + 0.12, wallTop - 0.004, WEST.z0, WEST.x1 - 0.02, wallTop + 0.004, WEST.z1 - 0.12, tint(STONE.tan, 0.9), 't');
  slab(s, FRONT.x0, wallTop - 0.004, FRONT.z0 + 0.09, FRONT.x1 - 0.02, wallTop + 0.004, FRONT.z1 - 0.12, tint(STONE.tan, 0.9), 't');

  // ---- keep top: sand deck and merlons all round ------------------------------------------------------------------------------------------------
  const kt = KEEP_WALL_TOP;
  slab(s, KEEP.x0 + 0.02, kt - 0.004, KEEP.z0 + 0.02, KEEP.x1 - 0.02, kt + 0.012, KEEP.z1 - 0.02, tint(STONE.sand, 0.95), 't');
  const kp = { y: kt, length: 0.14, gap: 0.09, h: 0.14, cap: 0.07 };
  merlons(ctx, { axis: 'x', fixed: KEEP.z0 + 0.05, from: KEEP.x0, to: KEEP.x1, ...kp });
  merlons(ctx, { axis: 'x', fixed: KEEP.z1 - 0.05, from: KEEP.x0, to: KEEP.x1, ...kp });
  merlons(ctx, { axis: 'z', fixed: KEEP.x0 + 0.05, from: KEEP.z0 + 0.1, to: KEEP.z1 - 0.1, ...kp });
  merlons(ctx, { axis: 'z', fixed: KEEP.x1 - 0.05, from: KEEP.z0 + 0.1, to: KEEP.z1 - 0.1, ...kp });

  // ---- courtyard retaining edge --------------------------------------------------------------------------------------------------------------
  slab(s, COURT.x0 - 0.01, H.floor - 0.06, COURT.z0 - 0.01, COURT.x1 + 0.01, H.floor, COURT.z1 + 0.01, tint(STONE.brown, 0.7), 't');
}
