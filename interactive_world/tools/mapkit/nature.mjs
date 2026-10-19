// Natural map pieces: trees, stone nodes, rocky outcrops (bump, cliff, slope), crop field, hay bales, fences, river bank stones.
// Every builder adds geometry around its own origin (footprint centre, y = 0 ground, front towards +z). T is one map tile in world units.
import { block, slab, beam, poly, tint, rock, mulberry32 } from '../lib/builder.mjs';
import { cylinder, cone, blob, lyingCylinder } from '../lib/solids.mjs';
import { pine, oak, bush } from './trees.mjs';
import { C } from './palette.mjs';

import { MODEL_TILE } from '../../src/world/config.js';
export const T = MODEL_TILE;   // one map tile as modelled (1.25 units); the map scales pieces to metres at placement (src/map/asset_scale.js)

const rockCol = (rng, k = 1) => tint(C.rock, (0.8 + rng() * 0.4) * k);

/** Stone resource node: a cluster of faceted boulders and slender spires. size 1..3 (tiles across). */
export function stoneNode(p, rng, { size = 1, spires = 0 } = {}) {
  const R = 0.34 * size * T, n = 3 + size * 2;
  for (let i = 0; i < n; i++) {
    const a = rng() * Math.PI * 2, d = rng() * R, s = (0.16 + rng() * 0.16) * T * (0.8 + 0.35 * size) * (1 - (d / R) * 0.3);
    rock(p.rock, Math.sin(a) * d, s * 0.35, Math.cos(a) * d, s * 1.15, s * (0.75 + rng() * 0.35), s, rng, rockCol(rng), { detail: 1, flat: 0.9 });
  }
  for (let i = 0; i < spires; i++) {
    const a = rng() * Math.PI * 2, d = rng() * R * 0.7, h = (0.5 + rng() * 0.5) * T * (0.6 + 0.4 * size), r = (0.17 + rng() * 0.1) * T;
    cylinder(p.rock, Math.sin(a) * d, 0, Math.cos(a) * d, r * 1.25, r * 0.6, h, 5, rockCol(rng, 1.05), { spin: rng() * 6, capTop: true });
  }
  for (let i = 0; i < 2 + size; i++) {
    const a = rng() * Math.PI * 2, d = R * (0.9 + rng() * 0.5);
    blob(p.leaf, Math.sin(a) * d, 0.06 * T, Math.cos(a) * d, 0.16 * T, 0.09 * T, 0.14 * T, tint(C.leaf2, 0.9 + rng() * 0.25), rng, 0);
  }
  if (size >= 2) pine(p, rng, { x: -R * 0.9, z: R * 0.5, h: (0.7 + 0.2 * size) * T });
}

/** Rocky bump: a grassy mound with boulders, a pine and bushes (2x2 tiles). */
export function rockyBump(p, rng) {
  blob(p.grass, 0, 0.0, 0, 0.88 * T, 0.28 * T, 0.8 * T, tint(C.grass, 0.95), rng, 1);
  for (let i = 0; i < 6; i++) {
    const a = rng() * Math.PI * 2, d = (0.15 + rng() * 0.45) * T, s = (0.2 + rng() * 0.2) * T;
    rock(p.rock, Math.sin(a) * d, 0.22 * T + s * 0.3, Math.cos(a) * d, s * 1.1, s * 0.9, s, rng, rockCol(rng), { detail: 1, flat: 0.5 });
  }
  pine(p, rng, { x: -0.5 * T, y: 0.2 * T, z: -0.35 * T, h: 1.0 * T });
  pine(p, rng, { x: 0.15 * T, y: 0.22 * T, z: -0.5 * T, h: 0.75 * T });
  for (const [x, z] of [[0.55, 0.45], [-0.55, 0.4], [0.7, -0.1]]) bush(p, rng, { x: x * T, y: 0.05 * T, z: z * T, s: 0.28 * T });
}

/** Cliff edge: a raised plateau of height `h` with a columnar rock face on the front and sides, grass and trees on top (w x d tiles). */
export function cliffEdge(p, rng, { w = 3, d = 2, h = 1.0 } = {}) {
  const hw = (w * T) / 2 - 0.1 * T, hd = (d * T) / 2 - 0.1 * T, H = h * T;
  block(p.rock, -hw + 0.12 * T, 0, -hd + 0.12 * T, hw - 0.12 * T, H - 0.03, hd - 0.12 * T, tint(C.rockDark, 0.9), { bevel: 0.01, faces: 'nsew' });
  const column = (x, z) => {
    const ch = H * (0.88 + rng() * 0.2), r = (0.1 + rng() * 0.05) * T;
    cylinder(p.rock, x, 0, z, r * 1.25, r * 1.0, ch, 6, rockCol(rng, 0.95), { spin: rng() * 6 });
  };
  for (let x = -hw; x <= hw; x += 0.19 * T) { column(x, hd - 0.04 * T + (rng() - 0.5) * 0.05 * T); if (rng() > 0.35) column(x + 0.08 * T, -hd + 0.06 * T); }
  for (let z = -hd + 0.15 * T; z < hd - 0.1 * T; z += 0.19 * T) { column(-hw + 0.03 * T, z); column(hw - 0.03 * T, z); }
  // grass top with a ragged edge
  const pts = [];
  for (let i = 0; i < 18; i++) { const a = (i / 18) * Math.PI * 2, k = 0.86 + rng() * 0.12; pts.push([Math.max(-hw, Math.min(hw, Math.sin(a) * hw * 1.15)) * k, H + 0.002, Math.max(-hd, Math.min(hd, Math.cos(a) * hd * 1.15)) * k]); }
  poly(p.grass, pts, tint(C.grass, 1.0));
  for (let i = 0; i < 5; i++) rock(p.rock, (rng() - 0.5) * hw * 1.5, H + 0.04 * T, (rng() - 0.5) * hd * 1.2, 0.12 * T, 0.09 * T, 0.1 * T, rng, rockCol(rng), { detail: 0, flat: 0.8 });
  for (let i = 0; i < Math.round(w); i++) (rng() > 0.5 ? pine : oak)(p, rng, { x: (rng() - 0.5) * hw * 1.5, y: H, z: (rng() - 0.5) * hd * 1.2 - 0.1 * T, h: (0.6 + rng() * 0.5) * T });
  for (let i = 0; i < 3; i++) bush(p, rng, { x: (rng() - 0.5) * hw * 1.7, y: H, z: hd * (0.2 + rng() * 0.5), s: 0.26 * T });
}

/** Slope piece: a grassy ramp rising to `h` at the back, rocks along one side (2x2 tiles). */
export function slopePiece(p, rng, { h = 0.7 } = {}) {
  const hw = 0.95 * T, hd = 0.95 * T, H = h * T;
  const A = [-hw, 0, hd], B = [hw, 0, hd], Cc = [hw, H, -hd], D = [-hw, H, -hd];
  p.grass.quad(A, B, Cc, D, tint(C.grass, 1.0));
  p.rock.quad([-hw, 0, hd], [-hw, H, -hd], [-hw, 0, -hd], [-hw, 0, -hd], rockCol(rng, 0.8));
  p.rock.tri([-hw, 0, hd], [-hw, H, -hd], [-hw, 0, -hd], rockCol(rng, 0.85));
  p.rock.tri([hw, 0, hd], [hw, 0, -hd], [hw, H, -hd], rockCol(rng, 0.9));
  for (let i = 0; i < 7; i++) {
    const t = rng(), z = hd - t * 2 * hd, y = H * t, s = (0.15 + rng() * 0.14) * T;
    rock(p.rock, -hw + 0.1 * T + rng() * 0.15 * T, y + s * 0.4, z, s * 1.2, s, s, rng, rockCol(rng), { detail: 1, flat: 0.6 });
  }
  pine(p, rng, { x: hw * 0.5, y: H * 0.8, z: -hd * 0.6, h: 0.8 * T });
  for (const [x, t] of [[0.2, 0.3], [-0.3, 0.6]]) bush(p, rng, { x: x * T, y: H * t, z: hd - t * 2 * hd, s: 0.24 * T });
}

/** Crop field: tilled soil with rows of golden wheat (2x2 tiles). Fences are separate pieces. */
export function cropField(p, rng, { w = 2, d = 2 } = {}) {
  const hw = (w * T) / 2 - 0.08 * T, hd = (d * T) / 2 - 0.08 * T;
  slab(p.dirt, -hw, 0, -hd, hw, 0.02, hd, tint(C.dirtDark, 0.9), 't');
  const rows = Math.round(w * 4.5), step = (2 * hw) / rows;
  for (let r = 0; r < rows; r++) {
    const x = -hw + step * (r + 0.5);
    for (let z = -hd + 0.06 * T; z < hd - 0.06 * T; z += 0.2 * T) {
      const hgt = (0.16 + rng() * 0.1) * T;
      block(p.thatch, x - step * 0.34, 0.02, z, x + step * 0.34, 0.02 + hgt, z + 0.17 * T, tint(r % 2 ? C.thatch : C.thatchDark, 0.92 + rng() * 0.2), { bevel: 0.012, faces: 'nsewt' });
    }
  }
}

/** Hay bale group: a grass pad with four round bales lying on their sides (2x2 tiles). */
export function hayBales(p, rng) {
  const hw = 0.95 * T;
  slab(p.grass, -hw, 0, -hw, hw, 0.02, hw, tint(C.grass, 0.95), 't');
  for (const [x, z, a] of [[-0.45, -0.4, 0.2], [0.35, -0.45, -0.3], [-0.4, 0.4, 0.5], [0.45, 0.35, 0.0]]) {
    p.thatch.with([Math.cos(a), 0, Math.sin(a), x * T, 0, 1, 0, 0, -Math.sin(a), 0, Math.cos(a), z * T, 0, 0, 0, 1], () => {
      lyingCylinder(p.thatch, 0, 0.3 * T, 0, 0.3 * T, 0.55 * T, 12, tint(C.thatch, 0.95 + rng() * 0.1), { capColor: tint(C.thatchDark, 1.0) });
    });
  }
}

/** One tile of wooden fence along x: two posts and two rails. */
export function fenceSegment(p, rng) {
  for (const x of [-0.5 * T, 0.5 * T]) block(p.wood, x - 0.03 * T, 0, -0.03 * T, x + 0.03 * T, 0.42 * T, 0.03 * T, tint(C.woodDark, 0.95 + rng() * 0.15), { bevel: 0.006 });
  for (const y of [0.17, 0.32]) beam(p.wood, [-0.5 * T, y * T, 0.0], [0.5 * T, y * T, 0.0], 0.03 * T, 0.045 * T, tint(C.wood, 0.95), [0, 1, 0]);
}

/** A few flat stones for river banks. */
export function bankStones(p, rng) {
  for (let i = 0; i < 4; i++) {
    const s = (0.1 + rng() * 0.1) * T;
    rock(p.rock, (i - 1.5) * 0.28 * T + (rng() - 0.5) * 0.1, s * 0.3, (rng() - 0.5) * 0.2 * T, s * 1.3, s * 0.8, s, rng, rockCol(rng, 1.0), { detail: 1, flat: 0.9 });
  }
}

/** Grass tuft: a handful of thin blades (two greens), sometimes with a flower; instanced by the thousand as ground cover. */
export function grassTuft(p, rng, { flower = false } = {}) {
  for (let i = 0; i < 6; i++) {
    const a = (i / 6) * Math.PI * 2 + rng(), d = 0.03 * T * (0.4 + rng()), h = (0.1 + rng() * 0.1) * T;
    cone(p.leaf, Math.sin(a) * d, 0, Math.cos(a) * d, 0.022 * T, h, 3, tint([C.leaf1, C.leaf3, C.leaf2][i % 3], 0.95 + rng() * 0.15), { spin: rng() * 6 });
  }
  if (flower) blob(p.leaf, 0.02 * T, 0.2 * T, 0, 0.032 * T, 0.03 * T, 0.032 * T, [0.97, 0.92, 0.55], rng, 0);
}

export { pine, oak, bush };
