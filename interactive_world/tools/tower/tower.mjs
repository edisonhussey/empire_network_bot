// The robber baron watchtower, built from modular parts (rock base, platform wall with arched door, shaft segments joined by timber bands,
// battlement with wooden floor, banners, ladder, props). Local frame: origin at the centre of the 3-unit footprint on the ground, y up,
// door and banners face +z. `segments` sets the height: total = (2.5 + segments) units (2 segments = the sheet's 4.5u).
import { block, slab, beam, poly, tint, RY, T4, rock } from '../lib/builder.mjs';
import { wallBox, stoneColor, STONE } from '../lib/masonry.mjs';
import { cylinder, cone, pyramid, blob, archRing } from '../lib/solids.mjs';
import { pine, bush } from '../mapkit/trees.mjs';
import { C } from '../mapkit/palette.mjs';

export const U = 0.95;            // world units per sheet unit
const PW = 1.12 * U;              // platform half width
const PZ0 = -1.0 * U, PZ1 = 1.02 * U;   // platform back / front
const SW = 0.66 * U;               // shaft half width
const SZ = -0.34 * U;             // shaft centre z (set back from the platform centre)

const ring = (p, x0, z0, x1, z1, y, h, col, grow) => block(p, x0 - grow, y, z0 - grow, x1 + grow, y + h, z1 + grow, col, { bevel: 0.012, faces: 'nsewt' });

/** A hanging banner (blue cloth, gold fleur-de-lis) on a vertical plane facing `facing` (0 = +z), top edge centred at (x, y, z). */
function banner(p, x, y, z, w, h, facing) {
  p.banner.with(T4(x, y, z), () => p.banner.with(RY(facing), () => {
    const col = [0.9, 0.9, 0.9];
    poly(p.banner, [[-w / 2, 0, 0], [w / 2, 0, 0], [w / 2, -h * 0.82, 0], [0, -h, 0], [-w / 2, -h * 0.82, 0]].reverse(), col);
  }));
  p.gold.with(T4(x, y, z), () => p.gold.with(RY(facing), () => {
    const cy = -h * 0.42, s = w * 0.2, z0 = 0.006;
    poly(p.gold, [[0, cy + s * 1.5, z0], [s * 0.55, cy + s * 0.2, z0], [0, cy - s * 0.2, z0], [-s * 0.55, cy + s * 0.2, z0]].reverse(), C.gold);
    poly(p.gold, [[s * 0.2, cy - s * 0.1, z0], [s * 1.1, cy + s * 0.5, z0], [s * 0.75, cy - s * 0.5, z0], [s * 0.1, cy - s * 1.1, z0]].reverse(), C.gold);
    poly(p.gold, [[-s * 0.2, cy - s * 0.1, z0], [-s * 0.1, cy - s * 1.1, z0], [-s * 0.75, cy - s * 0.5, z0], [-s * 1.1, cy + s * 0.5, z0]].reverse(), C.gold);
    poly(p.gold, [[-s * 0.75, cy - s * 1.15, z0], [s * 0.75, cy - s * 1.15, z0], [s * 0.75, cy - s * 1.35, z0], [-s * 0.75, cy - s * 1.35, z0]].reverse(), C.gold);
  }));
}

/** A timber band round a rectangle with metal straps and corbels under it. */
function timberBand(p, x0, z0, x1, z1, y, th = 0.15 * U, over = 0.06 * U) {
  ring(p.wood, x0, z0, x1, z1, y, th, tint(C.wood, 1.0), over);
  for (const [x, z] of [[x0, z1], [x1, z1], [x0, z0], [x1, z0]]) {
    const sx = x === x0 ? -1 : 1, sz = z === z1 ? 1 : -1;
    beam(p.wood, [x + sx * (over + 0.02), y, z + sz * (over + 0.02)], [x - sx * 0.04, y - th * 1.3, z - sz * 0.04], 0.07 * U, 0.07 * U, tint(C.woodDark, 1.1));
    block(p.metal, x + sx * over - 0.03, y + 0.01, z + sz * over - 0.03, x + sx * over + 0.03, y + th - 0.01, z + sz * over + 0.03, C.metal, { bevel: 0.006 });
  }
  const n = 4;
  for (let i = 1; i <= n; i++) {
    const t = i / (n + 1), x = x0 + (x1 - x0) * t;
    block(p.metal, x - 0.025, y + 0.01, z1 + over - 0.006, x + 0.025, y + th - 0.01, z1 + over + 0.012, C.metal, { bevel: 0.005 });
  }
}

export function buildTower(p, rng, { segments = 2, props = true } = {}) {
  const platformTop = 1.5 * U, rockTop = 0.5 * U;
  const shaftBottom = platformTop;
  const shaftTop = platformTop + segments * U;
  const top = shaftTop + U;
  const pillar = (x, z, h) => {
    const course = 0.12 * U;
    let y = rockTop, i = 0;
    while (y < h - 0.01) {
      const ch = Math.min(course, h - y), g = 0.012 + (i % 2) * 0.012;
      block(p.stone, x - 0.17 * U - g, y + 0.004, z - 0.17 * U - g, x + 0.17 * U + g, y + ch - 0.004, z + 0.17 * U + g, tint(stoneColor(rng, 'light'), 1.03), { bevel: 0.012, faces: 'nsewt' });
      y += ch; i++;
    }
    pyramid(p.wood, x, h, z, 0.22 * U, 0.22 * U, 0.2 * U, C.cap);
  };

  // ---- rock mound: irregular ring of rocks, mossy tops, a dirt path to the door --------------------------------------------------------------------
  const nRocks = 17;
  for (let i = 0; i < nRocks; i++) {
    const a = (i / nRocks) * Math.PI * 2 + rng() * 0.2;
    if (Math.abs(Math.sin(a / 2)) < 0.16) continue;                         // keep the path (towards +z) open
    const d = (1.45 + rng() * 0.28) * U * (Math.abs(Math.sin(a)) * 0.15 + 0.92);
    const s = (0.42 + rng() * 0.3) * U;
    rock(p.rock, Math.sin(a) * d, 0.12 * U + s * 0.22, Math.cos(a) * d * 0.96, s * 1.1, s * 0.78, s, rng, tint(C.rock, 0.82 + rng() * 0.35), { detail: 1, flat: 0.9 });
    if (rng() > 0.45) blob(p.leaf, Math.sin(a) * d * 0.96, 0.12 * U + s * 0.7, Math.cos(a) * d * 0.9, s * 0.55, s * 0.18, s * 0.5, tint(C.moss, 0.9 + rng() * 0.2), rng, 0);
  }
  // inner stepped rocks the platform sits on
  for (let i = 0; i < 9; i++) {
    const a = (i / 9) * Math.PI * 2 + 0.3, d = 1.12 * U;
    rock(p.rock, Math.sin(a) * d, 0.2 * U, Math.cos(a) * d, 0.5 * U, 0.4 * U, 0.46 * U, rng, tint(C.rock, 0.9 + rng() * 0.25), { detail: 1, flat: 0.9 });
  }
  // grass skirt and the path
  const octa = (r, y, col, ox = 0, oz = 0, n = 10) => { const pts = Array.from({ length: n }, (_, i) => { const a = (i / n) * Math.PI * 2, k = 0.9 + rng() * 0.2; return [ox + Math.sin(a) * r * k, y, oz + Math.cos(a) * r * k]; }); poly(p.grass, pts, col); };
  octa(1.55 * U, rockTop + 0.01, tint(C.grass, 0.95), 0, 0.05 * U);
  poly(p.dirt, [[-0.36 * U, rockTop + 0.02, 1.0 * U], [0.36 * U, rockTop + 0.02, 1.0 * U], [0.5 * U, rockTop + 0.02, 1.75 * U], [-0.5 * U, rockTop + 0.02, 1.75 * U]].reverse(), C.dirt);

  // ---- platform: walls with the arched door, solid top with a grass courtyard -------------------------------------------------------------------
  const doorW = 0.56 * U, doorH = 0.78 * U;
  wallBox(p.stone, {
    x0: -PW, z0: PZ0, x1: PW, z1: PZ1, y0: rockTop - 0.1 * U, y1: platformTop, rng, tone: 'light', thick: 0.14 * U, courseH: [0.085 * U, 0.13 * U], blockL: [0.16 * U, 0.34 * U],
    holes: { s: [{ u0: PW - doorW / 2, u1: PW + doorW / 2, v0: rockTop, v1: rockTop + doorH }] },
  });
  slab(p.stone, -PW + 0.02, platformTop - 0.02, PZ0 + 0.02, PW - 0.02, platformTop, PZ1 - 0.02, tint(C.stoneMid, 0.9), 't');
  poly(p.grass, [[-PW + 0.1 * U, platformTop + 0.006, PZ1 - 0.1 * U], [PW - 0.1 * U, platformTop + 0.006, PZ1 - 0.1 * U], [PW - 0.1 * U, platformTop + 0.006, PZ0 + 0.1 * U], [-PW + 0.1 * U, platformTop + 0.006, PZ0 + 0.1 * U]], tint(C.grass, 0.95));
  // door: dark interior, planks, iron straps, ring, stone arch
  const dz = PZ1 - 0.012;
  block(p.wood, -doorW / 2 + 0.01, rockTop, dz - 0.03 * U, doorW / 2 - 0.01, rockTop + doorH * 0.72, dz + 0.0, tint(C.plank, 0.95), { bevel: 0.006, faces: 's' });
  cylinder(p.wood, 0, rockTop + doorH * 0.72, dz - 0.0, doorW / 2 - 0.01, doorW / 2 - 0.01, 0.0001, 14, C.plank, { capTop: false });
  for (let i = 0; i < 14; i++) { // the round head of the door as a half fan
    const a0 = Math.PI * (i / 14), a1 = Math.PI * ((i + 1) / 14), r = doorW / 2 - 0.01, cy = rockTop + doorH * 0.72;
    p.wood.tri([0, cy, dz + 0.001], [Math.cos(a1) * r, cy + Math.sin(a1) * r, dz + 0.001], [Math.cos(a0) * r, cy + Math.sin(a0) * r, dz + 0.001], tint(C.plank, 0.9 + (i % 2) * 0.12));
  }
  for (const fy of [0.2, 0.55]) block(p.metal, -doorW / 2 + 0.01, rockTop + doorH * fy, dz + 0.0, doorW / 2 - 0.01, rockTop + doorH * fy + 0.05 * U, dz + 0.012, C.metal, { bevel: 0.004 });
  cylinder(p.gold, 0.0, rockTop + doorH * 0.34, dz + 0.018, 0.045 * U, 0.045 * U, 0.01, 10, tint(C.metal, 1.5), { capTop: true });
  archRing(p.stone, 0, rockTop + doorH * 0.72, dz - 0.04, doorW / 2, doorW / 2 + 0.16 * U, 0.1 * U, 10, tint(STONE.sand, 1.05), { z0: 0.0 });
  block(p.stone, -doorW / 2 - 0.16 * U, rockTop, dz - 0.03, -doorW / 2, rockTop + doorH * 0.72, dz + 0.07 * U, tint(STONE.sand, 1.0), { bevel: 0.01, faces: 'nsewt' });
  block(p.stone, doorW / 2, rockTop, dz - 0.03, doorW / 2 + 0.16 * U, rockTop + doorH * 0.72, dz + 0.07 * U, tint(STONE.sand, 1.0), { bevel: 0.01, faces: 'nsewt' });

  // low parapet walls and the wooden-capped pillars on the platform's front corners
  for (const sx of [-1, 1]) pillar(sx * (PW - 0.04 * U), PZ1 - 0.04 * U, platformTop + 0.55 * U);
  pillar(-PW + 0.04 * U, PZ0 + 0.1 * U, platformTop + 0.4 * U);
  wallBox(p.stone, { x0: -PW + 0.1 * U, z0: PZ1 - 0.18 * U, x1: -0.35 * U, z1: PZ1 - 0.04 * U, y0: platformTop, y1: platformTop + 0.32 * U, rng, tone: 'light', thick: 0.06 * U, sides: 's', courseH: [0.08 * U, 0.11 * U] });
  wallBox(p.stone, { x0: 0.35 * U, z0: PZ1 - 0.18 * U, x1: PW - 0.1 * U, z1: PZ1 - 0.04 * U, y0: platformTop, y1: platformTop + 0.32 * U, rng, tone: 'light', thick: 0.06 * U, sides: 's', courseH: [0.08 * U, 0.11 * U] });
  wallBox(p.stone, { x0: -PW + 0.04 * U, z0: PZ0 + 0.2 * U, x1: -PW + 0.18 * U, z1: PZ1 - 0.2 * U, y0: platformTop, y1: platformTop + 0.28 * U, rng, tone: 'light', thick: 0.06 * U, sides: 'w', courseH: [0.08 * U, 0.11 * U] });

  // ---- shaft: stone segments joined by timber bands, window slits ------------------------------------------------------------------------------
  const sx0 = -SW, sx1 = SW, sz0 = SZ - SW, sz1 = SZ + SW;
  timberBand(p, sx0, sz0, sx1, sz1, shaftBottom, 0.13 * U, 0.05 * U);
  for (let k = 0; k < segments; k++) {
    const y0 = shaftBottom + k * U + 0.13 * U, y1 = shaftBottom + (k + 1) * U;
    wallBox(p.stone, { x0: sx0, z0: sz0, x1: sx1, z1: sz1, y0, y1, rng, tone: 'light', thick: 0.12 * U, courseH: [0.085 * U, 0.12 * U], blockL: [0.14 * U, 0.3 * U] });
    // slit interiors and frames
    for (const [x, z, f] of [[0, sz1, 0], [sx1, SZ, Math.PI / 2], [sx0, SZ, (3 * Math.PI) / 2]]) {
      p.metal.with(T4(x, 0, z), () => p.metal.with(RY(f), () => {
        block(p.metal, -0.07 * U, y0 + 0.28 * U, 0.012, 0.07 * U, y0 + 0.62 * U, 0.02, tint(C.stoneDeep, 0.45), { bevel: 0.002, faces: 's' });
        block(p.stone, -0.11 * U, y0 + 0.24 * U, -0.012, -0.07 * U, y0 + 0.66 * U, 0.03, tint(STONE.sand, 1.05), { bevel: 0.006 });
        block(p.stone, 0.07 * U, y0 + 0.24 * U, -0.012, 0.11 * U, y0 + 0.66 * U, 0.03, tint(STONE.sand, 1.05), { bevel: 0.006 });
        block(p.stone, -0.11 * U, y0 + 0.62 * U, -0.012, 0.11 * U, y0 + 0.69 * U, 0.03, tint(STONE.sand, 1.1), { bevel: 0.006 });
      }));
    }
    if (k < segments - 1) timberBand(p, sx0, sz0, sx1, sz1, y1 - 0.0, 0.13 * U, 0.05 * U);
  }

  // ---- battlement: overhanging timber band, crenellated parapet, wooden floor, pillar caps -----------------------------------------------------
  const bx0 = sx0 - 0.1 * U, bx1 = sx1 + 0.1 * U, bz0 = sz0 - 0.1 * U, bz1 = sz1 + 0.1 * U;
  timberBand(p, sx0, sz0, sx1, sz1, shaftTop - 0.0, 0.14 * U, 0.1 * U);
  const bodyTop = shaftTop + 0.14 * U + 0.55 * U;
  wallBox(p.stone, { x0: bx0, z0: bz0, x1: bx1, z1: bz1, y0: shaftTop + 0.14 * U, y1: bodyTop, rng, tone: 'light', thick: 0.12 * U, courseH: [0.09 * U, 0.13 * U], blockL: [0.16 * U, 0.3 * U],
  });
  p.metal.with(T4(0, 0, bz1), () => {
    block(p.metal, -0.08 * U, shaftTop + 0.34 * U, 0.012, 0.08 * U, shaftTop + 0.62 * U, 0.02, tint(C.stoneDeep, 0.45), { bevel: 0.002, faces: 's' });
    block(p.stone, -0.12 * U, shaftTop + 0.3 * U, 0.0, -0.08 * U, shaftTop + 0.66 * U, 0.034, tint(STONE.sand, 1.05), { bevel: 0.006 });
    block(p.stone, 0.08 * U, shaftTop + 0.3 * U, 0.0, 0.12 * U, shaftTop + 0.66 * U, 0.034, tint(STONE.sand, 1.05), { bevel: 0.006 });
    block(p.stone, -0.12 * U, shaftTop + 0.62 * U, 0.0, 0.12 * U, shaftTop + 0.69 * U, 0.034, tint(STONE.sand, 1.1), { bevel: 0.006 });
  });
  // wooden floor, visible from above
  slab(p.wood, bx0 + 0.05 * U, bodyTop - 0.06 * U, bz0 + 0.05 * U, bx1 - 0.05 * U, bodyTop - 0.01 * U, bz1 - 0.05 * U, tint(C.plank, 0.95), 't');
  const planks = 6;
  for (let i = 1; i < planks; i++) {
    const z = bz0 + 0.05 * U + ((bz1 - bz0 - 0.1 * U) * i) / planks;
    block(p.wood, bx0 + 0.06 * U, bodyTop - 0.012, z - 0.004, bx1 - 0.06 * U, bodyTop - 0.004, z + 0.004, tint(C.woodDark, 0.9), { bevel: 0.001, faces: 't' });
  }
  // merlons round the rim, corner pillars with timber caps
  const mh = 0.3 * U, mw = 0.2 * U;
  const side = (a0, a1, fixed, axis) => {
    const n = Math.max(2, Math.round((a1 - a0) / (mw * 1.9)));
    for (let i = 0; i < n; i++) {
      const u = a0 + 0.05 * U + ((a1 - a0 - 0.1 * U - mw) * (n === 1 ? 0 : i)) / Math.max(1, n - 1);
      const [x0, z0, x1, z1] = axis === 'x' ? [u, fixed - 0.06 * U, u + mw, fixed + 0.06 * U] : [fixed - 0.06 * U, u, fixed + 0.06 * U, u + mw];
      block(p.stone, x0, bodyTop, z0, x1, bodyTop + mh, z1, tint(stoneColor(rng, 'light'), 1.05), { bevel: 0.012, faces: 'nsewt' });
    }
  };
  block(p.stone, bx0, bodyTop - 0.02, bz0, bx1, bodyTop + 0.06 * U, bz1, tint(C.stoneMid, 0.8), { bevel: 0.01, faces: 'nsew' });
  side(bx0 + 0.2 * U, bx1 - 0.2 * U, bz1 - 0.06 * U, 'x'); side(bx0 + 0.2 * U, bx1 - 0.2 * U, bz0 + 0.06 * U, 'x');
  side(bz0 + 0.2 * U, bz1 - 0.2 * U, bx0 + 0.06 * U, 'z'); side(bz0 + 0.2 * U, bz1 - 0.2 * U, bx1 - 0.06 * U, 'z');
  for (const [x, z] of [[bx0, bz0], [bx1, bz0], [bx0, bz1], [bx1, bz1]]) {
    const sx = x === bx0 ? 1 : -1, sz = z === bz0 ? 1 : -1;
    block(p.stone, Math.min(x, x + sx * 0.3 * U), shaftTop + 0.14 * U, Math.min(z, z + sz * 0.3 * U), Math.max(x, x + sx * 0.3 * U), bodyTop + mh + 0.1 * U, Math.max(z, z + sz * 0.3 * U), tint(STONE.sand, 1.08), { bevel: 0.014, faces: 'nsewt' });
    pyramid(p.wood, x + sx * 0.15 * U, bodyTop + mh + 0.1 * U, z + sz * 0.15 * U, 0.2 * U, 0.2 * U, 0.2 * U, C.cap);
  }

  // ---- banners: two hang from the top band; a pole with a banner and a spearhead stands on each platform corner ------------------------------------
  const bannerTop = shaftTop + 0.12 * U;
  banner(p, SW * 0.5, bannerTop, sz1 + 0.12 * U, 0.42 * U, 1.35 * U, 0);
  banner(p, -SW * 0.5, bannerTop, sz1 + 0.12 * U, 0.42 * U, 1.35 * U, 0);
  banner(p, sx1 + 0.12 * U, bannerTop, SZ, 0.42 * U, 1.3 * U, Math.PI / 2);
  for (const sx of [-1, 1]) {
    const px = sx * (PW + 0.05 * U), pz = PZ1 - 0.1 * U, ph = platformTop + 1.3 * U;
    beam(p.wood, [px, rockTop, pz], [px, ph, pz], 0.06 * U, 0.06 * U, C.woodDark);
    cone(p.metal, px, ph, pz, 0.05 * U, 0.2 * U, 4, C.metal);
    banner(p, px + sx * 0.2 * U, ph - 0.12 * U, pz, 0.34 * U, 0.7 * U, sx > 0 ? Math.PI / 2 : (3 * Math.PI) / 2);
  }

  // ---- ladder up the right side of the platform ------------------------------------------------------------------------------------------------------
  {
    const lx = PW + 0.22 * U, z0 = PZ1 - 0.5 * U, z1 = PZ1 - 1.15 * U;
    for (const dz of [-0.09 * U, 0.09 * U]) beam(p.wood, [lx + 0.5 * U, rockTop, z0 - dz], [lx - 0.04 * U, platformTop + 0.18 * U, z1 - dz + 0.35 * U], 0.045 * U, 0.045 * U, C.woodLight, [0, 1, 0]);
    // rungs between the rails
    for (let i = 1; i < 8; i++) {
      const t = i / 8, x = lx + 0.5 * U + (-0.04 * U - 0.5 * U) * t, y = rockTop + (platformTop + 0.18 * U - rockTop) * t, z = z0 + (z1 + 0.35 * U - z0) * t;
      beam(p.wood, [x, y, z - 0.09 * U], [x, y, z + 0.09 * U], 0.035 * U, 0.035 * U, C.woodLight);
    }
  }

  // ---- props: pines either side, bushes, crates and a barrel in the courtyard --------------------------------------------------------------------------
  if (props) {
    pine(p, rng, { x: -1.35 * U, y: rockTop, z: 0.25 * U, h: 1.5 * U });
    pine(p, rng, { x: -1.0 * U, y: rockTop, z: -0.85 * U, h: 1.15 * U });
    pine(p, rng, { x: 1.38 * U, y: rockTop, z: -0.6 * U, h: 1.3 * U });
    pine(p, rng, { x: 1.3 * U, y: rockTop, z: 0.55 * U, h: 0.9 * U });
    for (const [x, z] of [[-1.0, 1.2], [0.95, 1.35], [1.1, 0.2], [-0.7, 1.55], [0.3, 1.6]]) bush(p, rng, { x: x * U, y: rockTop, z: z * U, s: 0.28 * U });
    for (const [x, z, s] of [[-0.7, 0.55, 0.28], [-0.45, 0.62, 0.22]]) {
      block(p.wood, x * U - s * U / 2, platformTop, z * U - s * U / 2, x * U + s * U / 2, platformTop + s * U, z * U + s * U / 2, tint(C.plank, 0.95), { bevel: 0.008 });
      beam(p.wood, [x * U - s * U / 2, platformTop + s * U + 0.004, z * U], [x * U + s * U / 2, platformTop + s * U + 0.004, z * U], 0.025 * U, 0.01, C.woodDark);
    }
    cylinder(p.wood, 0.55 * U, platformTop, 0.62 * U, 0.12 * U, 0.12 * U, 0.26 * U, 9, C.woodLight);
    for (const fy of [0.07, 0.19]) cylinder(p.metal, 0.55 * U, platformTop + fy * U, 0.62 * U, 0.126 * U, 0.126 * U, 0.025 * U, 9, C.metal, { capTop: false });
    blob(p.leaf, -0.2 * U, platformTop + 0.05 * U, PZ1 - 0.35 * U, 0.2 * U, 0.1 * U, 0.15 * U, C.leaf1, rng, 0);
  }
  return { top, platformTop };
}
