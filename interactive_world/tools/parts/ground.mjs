// Ground: the grass island with its soil skirt, boulders round the castle, courtyard paving, moss.
import { rock, tile, tint, mix, poly, slab } from '../lib/builder.mjs';
import { STONE } from '../lib/masonry.mjs';
import { H, COURT, TOWER } from './layout.mjs';

const CX = 1.25, CZ = 1.02;

export function buildGround(ctx) {
  const { prims, P, rng } = ctx;
  const g = prims.grass, r = prims.rock;

  // ---- island outline: a lumpy ellipse; grass on top, a soil/rock skirt below ----------------------------------------------------------------
  const N = 40;
  const outline = [];
  for (let i = 0; i < N; i++) {
    const a = (i / N) * Math.PI * 2;
    const wob = 1 + 0.07 * Math.sin(a * 3 + 1) + 0.04 * Math.sin(a * 7) + (rng() - 0.5) * 0.04;
    outline.push([CX + Math.sin(a) * 1.78 * wob, CZ + Math.cos(a) * 1.5 * wob]);
  }
  const grassTop = tint(P.green, 1.0);
  const soil = tint(P.brown, 0.85);
  for (let i = 0; i < N; i++) {
    const [x0, z0] = outline[i], [x1, z1] = outline[(i + 1) % N];
    const dip = (a, b) => -0.07 - 0.03 * Math.sin(a * 5 + b);
    const mx0 = CX + (x0 - CX) * 0.9, mz0 = CZ + (z0 - CZ) * 0.9, mx1 = CX + (x1 - CX) * 0.9, mz1 = CZ + (z1 - CZ) * 0.9;
    // skirt: two bands, lighter at the lip, darker below
    g.quad([x0, 0, z0], [mx0, dip(i, 1), mz0], [mx1, dip(i + 1, 1), mz1], [x1, 0, z1], tint(soil, 1.25 + rng() * 0.1));
    g.quad([mx0, dip(i, 1), mz0], [CX + (x0 - CX) * 0.7, -0.17, CZ + (z0 - CZ) * 0.7], [CX + (x1 - CX) * 0.7, -0.17, CZ + (z1 - CZ) * 0.7], [mx1, dip(i + 1, 1), mz1], tint(soil, 0.8));
    // top: a fan to the centre, a little brighter towards the rim
    const c = tint(grassTop, 0.9 + rng() * 0.18);
    g.tri([CX, 0, CZ], [x0, 0, z0], [x1, 0, z1], c);
  }
  // flat-ish underside so the island is closed
  for (let i = 0; i < N; i++) {
    const [x0, z0] = outline[i], [x1, z1] = outline[(i + 1) % N];
    g.tri([CX, -0.17, CZ], [CX + (x1 - CX) * 0.7, -0.17, CZ + (z1 - CZ) * 0.7], [CX + (x0 - CX) * 0.7, -0.17, CZ + (z0 - CZ) * 0.7], soil);
  }

  // ---- boulders: a loose ring of grey rocks round the walls and the tower --------------------------------------------------------------------
  const rockCol = tint(P.rock, 0.78);
  const place = (x, z, s, cols = rockCol) => rock(r, x, 0.02 + s * 0.28, z, s * 1.1, s * 0.75, s, rng, cols, { detail: 1, flat: 0.9 });
  const spots = [[0.35, 1.97, 0.16], [0.02, 1.9, 0.14], [-0.12, 1.4, 0.2], [-0.1, 0.95, 0.15], [0.05, 0.5, 0.18], [-0.05, 0.1, 0.2],
    [0.3, -0.12, 0.17], [0.9, -0.12, 0.2], [1.5, -0.08, 0.16], [2.05, 0.05, 0.19], [2.45, 0.3, 0.14], [2.6, 1.55, 0.17],
    [2.35, 1.82, 0.2], [1.95, 2.02, 0.15], [1.65, 2.14, 0.17], [1.12, 2.2, 0.13], [0.68, 2.16, 0.18]];
  for (const [x, z, s] of spots) place(x, z, s, tint(P.rock, 0.66 + rng() * 0.2));
  // small pebbles and grass tufts
  for (let i = 0; i < 26; i++) {
    const a = rng() * Math.PI * 2, d = 1.25 + rng() * 0.4;
    const x = CX + Math.sin(a) * d * 1.15, z = CZ + Math.cos(a) * d * 0.95;
    if (x > -0.05 && x < 2.55 && z > -0.05 && z < 2.05) continue; // not under the castle
    rock(r, x, 0.015, z, 0.035 + rng() * 0.03, 0.025, 0.035 + rng() * 0.03, rng, tint(P.rock, 0.8 + rng() * 0.3), { detail: 0, flat: 0.8 });
  }

  // ---- courtyard paving: little pillowed flagstones with a few grass patches --------------------------------------------------------------------
  const s = prims.stone;
  const step = 0.13;
  for (let x = COURT.x0 + 0.015; x < COURT.x1 - step * 0.6; x += step) {
    for (let z = COURT.z0 + 0.015; z < COURT.z1 - step * 0.6; z += step * 0.85) {
      const grassy = (Math.sin(x * 9) + Math.cos(z * 7 + x * 2)) > 1.15;
      if (grassy) continue;
      const col = tint(mix(STONE.tan, STONE.sand, rng() * 0.6), 0.88 + rng() * 0.18);
      tile(s, [x, H.floor, z], [step - 0.012, 0, 0], [0, 0, step * 0.85 - 0.012], col, [0, 1, 0], 0.01, 0.006);
    }
  }
  // grass inlay under the paving gaps
  slab(g, COURT.x0 + 0.005, H.floor - 0.03, COURT.z0 + 0.005, COURT.x1 - 0.005, H.floor + 0.004, COURT.z1 - 0.005, tint(P.moss, 0.9), 't');

  // ---- moss on the lowest course and tufts at the wall feet --------------------------------------------------------------------------------------
  for (let i = 0; i < 18; i++) {
    const x = 0.1 + rng() * 2.2, z = 1.95 + rng() * 0.12;
    poly(g, [[x - 0.05, 0.012, z + 0.05], [x + 0.05, 0.012, z + 0.05], [x + 0.04, 0.012, z - 0.04], [x - 0.04, 0.012, z - 0.05]], tint(P.moss, 0.85 + rng() * 0.3));
  }
}
