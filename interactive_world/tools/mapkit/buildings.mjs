// Map buildings: houses (thatch or red tile), farm building, small watchtower. Origin = footprint centre, y = 0, front towards +z.
import { block, slab, beam, poly, tint } from '../lib/builder.mjs';
import { cylinder, cone, pyramid, blob } from '../lib/solids.mjs';
import { tileGrid } from './roofgrid.mjs';
import { bush, pine } from './trees.mjs';
import { C } from './palette.mjs';
import { T } from './nature.mjs';

/** Gable roof along x over a rectangle: two slopes of thatch / tile, barge boards and plaster gable ends. */
function gableRoof(p, rng, { x0, x1, z0, z1, y, rise, over = 0.18, kind = 'thatch' }) {
  const hz = (z1 - z0) / 2, zc = (z0 + z1) / 2, run = hz + over, slope = rise / hz;
  const yEave = y - over * slope;
  const prim = kind === 'thatch' ? p.thatch : p.tile;
  const colors = kind === 'thatch' ? [C.thatch, C.thatch, C.thatchDark, tint(C.thatch, 1.1)] : [C.roofRed, C.roofRed, C.roofRedDark, tint(C.roofRed, 1.1)];
  const under = kind === 'thatch' ? C.thatchDark : C.roofRedDark;
  const ox = x0 - over * 0.6, len = x1 - x0 + over * 1.2;
  const o = { rowH: kind === 'thatch' ? 0.12 : 0.1, tileW: kind === 'thatch' ? 0.18 : 0.15, colors, under, bevel: kind === 'thatch' ? 0.016 : 0.008, lift: kind === 'thatch' ? 0.03 : 0.014 };
  tileGrid(prim, rng, [ox, yEave, zc + run], [len, 0, 0], [0, y + rise - yEave - 0.0, -run], o);
  tileGrid(prim, rng, [ox + len, yEave, zc - run], [-len, 0, 0], [0, y + rise - yEave, run], o);
  beam(p.wood, [ox - 0.02, y + rise + 0.015, zc], [ox + len + 0.02, y + rise + 0.015, zc], 0.08, 0.06, kind === 'thatch' ? tint(C.thatchDark, 0.9) : C.roofRedDark);
  for (const x of [x0, x1]) { // plaster gable fill with a timber frame
    poly(p.stone, [[x, y, z1], [x, y, z0], [x, y + rise, zc]].map((q) => q).reverse().reverse(), tint(C.plaster, 0.95));
    beam(p.wood, [x, y, z1], [x, y + rise, zc], 0.05, 0.05, C.woodDark, [1, 0, 0]);
    beam(p.wood, [x, y, z0], [x, y + rise, zc], 0.05, 0.05, C.woodDark, [1, 0, 0]);
  }
}

function plasterBox(p, rng, { x0, z0, x1, z1, y0 = 0, h }) {
  block(p.stone, x0 - 0.03, y0, z0 - 0.03, x1 + 0.03, y0 + 0.14, z1 + 0.03, tint(C.stoneDark, 0.95), { bevel: 0.01 });       // stone footing
  block(p.stone, x0, y0 + 0.14, z0, x1, y0 + h, z1, tint(C.plaster, 0.96), { bevel: 0.008, faces: 'nsew' });                 // plaster walls
  const top = y0 + h;
  for (const x of [x0, (x0 + x1) / 2, x1]) for (const z of [z0, z1]) block(p.wood, x - 0.025, y0 + 0.14, z - 0.025, x + 0.025, top, z + 0.025, C.woodDark, { bevel: 0.004 });
  for (const z of [z0, z1]) beam(p.wood, [x0, y0 + 0.14 + (h - 0.14) * 0.55, z], [x1, y0 + 0.14 + (h - 0.14) * 0.55, z], 0.045, 0.045, C.woodDark, [0, 1, 0]);
  beam(p.wood, [x0, top - 0.02, z1], [x1, top - 0.02, z1], 0.06, 0.06, C.woodDark, [0, 1, 0]);
}

function doorAndWindows(p, { x0, x1, z1, y0 = 0, h }) {
  const cx = (x0 + x1) / 2;
  block(p.wood, cx - 0.13, y0 + 0.14, z1 - 0.01, cx + 0.13, y0 + 0.14 + 0.46, z1 + 0.025, tint(C.plank, 0.85), { bevel: 0.006, faces: 'nsewt' });
  block(p.wood, cx - 0.16, y0 + 0.14 + 0.46, z1 - 0.012, cx + 0.16, y0 + 0.14 + 0.5, z1 + 0.03, C.woodDark, { bevel: 0.004 });
  for (const sx of [-1, 1]) {
    const wx = cx + sx * (x1 - x0) * 0.31, wy = y0 + 0.14 + (h - 0.14) * 0.42;
    block(p.metal, wx - 0.08, wy, z1 - 0.005, wx + 0.08, wy + 0.17, z1 + 0.012, tint(C.stoneDeep, 0.6), { bevel: 0.003, faces: 's' });
    block(p.wood, wx - 0.1, wy - 0.025, z1 - 0.01, wx + 0.1, wy + 0.2, z1 + 0.01, C.woodDark, { bevel: 0.004, faces: 'nsew' });
    block(p.wood, wx - 0.12, wy - 0.01, z1 - 0.01, wx - 0.085, wy + 0.18, z1 + 0.025, tint(C.blue, 0.9), { bevel: 0.003 });   // shutters
    block(p.wood, wx + 0.085, wy - 0.01, z1 - 0.01, wx + 0.12, wy + 0.18, z1 + 0.025, tint(C.blue, 0.9), { bevel: 0.003 });
  }
}

/** House (2x2 tiles). kind: 'thatch' | 'tile'. */
export function house(p, rng, { kind = 'thatch' } = {}) {
  const x0 = -0.62 * T, x1 = 0.62 * T, z0 = -0.46 * T, z1 = 0.46 * T, h = 0.74 * T;
  plasterBox(p, rng, { x0, z0, x1, z1, h });
  doorAndWindows(p, { x0, x1, z1, h });
  gableRoof(p, rng, { x0, x1, z0, z1, y: h, rise: 0.5 * T, over: 0.16 * T, kind });
  block(p.stone, x1 - 0.28 * T, h - 0.1, z0 + 0.05, x1 - 0.1 * T, h + 0.62 * T, z0 + 0.25 * T, tint(C.stoneMid, 0.9), { bevel: 0.012 });   // chimney
  block(p.stone, x1 - 0.31 * T, h + 0.62 * T, z0 + 0.02, x1 - 0.07 * T, h + 0.68 * T, z0 + 0.28 * T, tint(C.stoneDark, 0.8), { bevel: 0.008 });
  bush(p, rng, { x: x0 - 0.12 * T, y: 0, z: z1 + 0.05 * T, s: 0.3 * T }); bush(p, rng, { x: x1 + 0.1 * T, y: 0, z: z1 + 0.18 * T, s: 0.26 * T });
  block(p.wood, x0 - 0.1 * T, 0, z1 + 0.28 * T, x0 + 0.1 * T, 0.16 * T, z1 + 0.5 * T, C.plank, { bevel: 0.008 });                                  // woodpile / bench
}

/** Farm building (3x2 tiles): a long thatched barn-house with a porch, hay and crates. */
export function farmBuilding(p, rng) {
  const x0 = -0.95 * T, x1 = 0.95 * T, z0 = -0.5 * T, z1 = 0.5 * T, h = 0.8 * T;
  plasterBox(p, rng, { x0, z0, x1, z1, h });
  doorAndWindows(p, { x0, x1, z1, h });
  gableRoof(p, rng, { x0, x1, z0, z1, y: h, rise: 0.62 * T, over: 0.2 * T, kind: 'thatch' });
  block(p.stone, x0 + 0.1 * T, h - 0.1, z0 + 0.05, x0 + 0.3 * T, h + 0.7 * T, z0 + 0.25 * T, tint(C.stoneMid, 0.9), { bevel: 0.012 });
  for (const [x, z] of [[1.15, 0.5], [1.3, 0.2], [1.05, 0.1]]) block(p.wood, x * T - 0.14 * T, 0, z * T - 0.14 * T, x * T + 0.14 * T, 0.28 * T, z * T + 0.14 * T, tint(C.plank, 0.9 + rng() * 0.2), { bevel: 0.008 });
  blob(p.thatch, -1.2 * T, 0.2 * T, 0.45 * T, 0.28 * T, 0.28 * T, 0.28 * T, C.thatch, rng, 1);
  bush(p, rng, { x: -0.6 * T, z: 0.72 * T, s: 0.3 * T });
}

/** Small watchtower (2x2 tiles): stone shaft with blue banners, a timber gallery, a red pyramid roof and a flag. */
export function watchtowerSmall(p, rng, { height = 1.0 } = {}) {
  const s = 0.34 * T, base = 0.1 * T, shaftH = 1.45 * T * height, gy = base + shaftH;
  block(p.stone, -s - 0.06, 0, -s - 0.06, s + 0.06, base, s + 0.06, tint(C.stoneDark, 0.95), { bevel: 0.012 });
  for (let i = 0, y = base; y < gy - 0.01; i++) {                              // stone courses
    const ch = Math.min(0.12 * T, gy - y), g = (i % 2) * 0.008;
    block(p.stone, -s - g, y + 0.003, -s - g, s + g, y + ch - 0.003, s + g, tint(C.stoneMid, 0.92 + (i % 3) * 0.06), { bevel: 0.008, faces: 'nsewt' });
    y += ch;
  }
  for (const [fx, fz, yaw] of [[0, 1, 0], [1, 0, 1], [-1, 0, 2]]) {            // blue banner panels and a slit
    p.banner.with([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1], () => {});
    const bx = fx * (s + 0.012), bz = fz * (s + 0.012), w = 0.18 * T, y0 = base + shaftH * 0.38, y1 = base + shaftH * 0.85;
    const [ax, az, ex, ez] = fz ? [-w, 0, w, 0] : [0, -w, 0, w];
    p.banner.quad([bx + ax, y0, bz + az], [bx + ex, y0, bz + ez], [bx + ex, y1, bz + ez], [bx + ax, y1, bz + az], [0.9, 0.9, 0.9]);
    p.gold.quad([bx * 1.004 + ax * 0.3, y0 + (y1 - y0) * 0.5, bz * 1.004 + az * 0.3], [bx * 1.004 + ex * 0.3, y0 + (y1 - y0) * 0.5, bz * 1.004 + ez * 0.3], [bx * 1.004 + ex * 0.3, y0 + (y1 - y0) * 0.72, bz * 1.004 + ez * 0.3], [bx * 1.004 + ax * 0.3, y0 + (y1 - y0) * 0.72, bz * 1.004 + az * 0.3], C.gold);
  }
  // gallery: a wider timber box on corbels with posts, then the roof
  const g = s + 0.14 * T;
  block(p.wood, -g, gy, -g, g, gy + 0.1 * T, g, tint(C.wood, 1.0), { bevel: 0.01 });
  for (const sx of [-1, 1]) for (const sz of [-1, 1]) {
    beam(p.wood, [sx * s, gy - 0.12 * T, sz * s], [sx * g, gy, sz * g], 0.05 * T, 0.05 * T, C.woodDark);
    block(p.wood, sx * g - 0.03 * T, gy + 0.1 * T, sz * g - 0.03 * T, sx * g + 0.03 * T, gy + 0.55 * T, sz * g + 0.03 * T, C.woodDark, { bevel: 0.006 });
  }
  for (const y of [0.25, 0.42]) block(p.wood, -g, gy + y * T, -g, g, gy + (y + 0.04) * T, g, tint(C.wood, 0.9), { bevel: 0.004, faces: 'nsew' }), 0;
  block(p.wood, -s + 0.02, gy + 0.1 * T, -s + 0.02, s - 0.02, gy + 0.45 * T, s - 0.02, tint(C.woodDark, 1.1), { bevel: 0.006 });
  pyramid(p.tile, 0, gy + 0.55 * T, 0, g + 0.08 * T, g + 0.08 * T, 0.5 * T, C.roofRed);
  beam(p.wood, [0, gy + 0.55 * T, 0], [0, gy + 1.45 * T, 0], 0.03 * T, 0.03 * T, C.woodDark);
  p.banner.quad([0.015, gy + 1.42 * T, 0], [0.015, gy + 1.12 * T, 0], [0.015, gy + 1.12 * T, 0.0], [0.015, gy + 1.42 * T, 0], [0.9, 0.9, 0.9]);
  p.banner.quad([0, gy + 1.42 * T, 0.0], [0, gy + 1.42 * T, 0.5 * T], [0, gy + 1.2 * T, 0.5 * T], [0, gy + 1.2 * T, 0.0], [0.9, 0.9, 0.9]);
  bush(p, rng, { x: -0.55 * T, z: 0.5 * T, s: 0.3 * T }); bush(p, rng, { x: 0.55 * T, z: 0.55 * T, s: 0.26 * T });
  pine(p, rng, { x: -0.8 * T, z: -0.4 * T, h: 0.9 * T });
}
