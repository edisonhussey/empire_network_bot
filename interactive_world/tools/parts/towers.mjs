// The round tower on its rock plinth (blue straps, slit windows, corbelled crown, merlons, conical blue roof).
import { block, rock, tint, T4, RY } from '../lib/builder.mjs';
import { roundTower, stoneColor, STONE } from '../lib/masonry.mjs';
import { TOWER } from './layout.mjs';

const { cx, cz, r } = TOWER;
const BODY_Y0 = 0.22, BODY_Y1 = 1.6;
const pt = (a, y, rad) => [cx + Math.sin(a) * rad, y, cz + Math.cos(a) * rad];

// Slit windows: angle (0 = +z, towards the viewer), bottom, top.
const SLITS = [
  [0.1, 0.92, 1.18], [1.25, 0.92, 1.18], [-1.1, 0.92, 1.18], [2.2, 0.92, 1.18],
  [-0.45, 1.3, 1.5], [0.7, 1.3, 1.5], [1.8, 1.3, 1.5],
];
const SLIT_HALF = 0.07; // radians

/** A cylinder band (blue strap): `n` outward-facing quads between two heights. */
function band(prim, rad, y0, y1, color, n = 24) {
  for (let i = 0; i < n; i++) {
    const a0 = (i / n) * Math.PI * 2, a1 = ((i + 1) / n) * Math.PI * 2;
    prim.quad(pt(a0, y0, rad), pt(a1, y0, rad), pt(a1, y1, rad), pt(a0, y1, rad), tint(color, 0.92 + (i % 2) * 0.1));
    prim.quad(pt(a0, y1, rad), pt(a1, y1, rad), pt(a1, y1, rad - 0.02), pt(a0, y1, rad - 0.02), tint(color, 1.2));
  }
}

export function buildTowers(ctx) {
  const { prims, P, rng } = ctx;
  const s = prims.stone, rk = prims.rock;

  // ---- rock plinth: fat boulders at the foot with fang-like standing stones between them ------------------------------------------------------
  const dark = tint(P.rock, 0.55);
  for (let i = 0; i < 9; i++) {
    const a = 0.2 + (i / 9) * (Math.PI * 1.7) - 0.4; // gap towards the hall (-z) where the tower meets it
    const d = r + 0.04 + rng() * 0.06;
    rock(rk, cx + Math.sin(a) * d, 0.12, cz + Math.cos(a) * d, 0.16 + rng() * 0.05, 0.16, 0.15 + rng() * 0.05, rng, tint(dark, 0.9 + rng() * 0.3), { detail: 1, flat: 0.8 });
  }
  for (let i = 0; i < 10; i++) {
    const a = -0.6 + (i / 10) * (Math.PI * 1.9);
    const d = r + 0.03 + (i % 2) * 0.08;
    const h = 0.22 + rng() * 0.22;
    rock(rk, cx + Math.sin(a) * d, 0.1 + h * 0.6, cz + Math.cos(a) * d, 0.055 + rng() * 0.02, h, 0.05 + rng() * 0.02, rng, tint(dark, 1.0 + rng() * 0.35), { detail: 0, flat: 0 });
  }
  // a collar ring under the tower body
  for (let i = 0; i < 14; i++) {
    const a = (i / 14) * Math.PI * 2;
    const chord = 2 * (r + 0.07) * Math.sin(Math.PI / 14) - 0.01;
    s.with(T4(...pt(a, 0, r + 0.07)), () => s.with(RY(a), () => block(s, -chord / 2, 0.1, -0.1, chord / 2, BODY_Y0 + 0.12, 0.0, tint(STONE.dark, 0.9 + rng() * 0.2), { bevel: 0.016, faces: 'stewn' })));
  }

  // ---- body -----------------------------------------------------------------------------------------------------------------------------------
  roundTower(s, {
    cx, cz, r, y0: BODY_Y0 + 0.1, y1: BODY_Y1, rng, tone: 'mid', thick: 0.13, courseH: [0.08, 0.12], blockL: [0.14, 0.27],
    holes: SLITS.map(([a, v0, v1]) => ({ a0: a - SLIT_HALF, a1: a + SLIT_HALF, v0, v1 })),
  });
  // blue straps with a lighter top lip
  band(prims.roof, r + 0.018, 0.78, 0.86, P.blue2);
  band(prims.roof, r + 0.018, 1.46, 1.54, P.blue2);
  // slit windows: tan frame bars either side and a glowing pane set back
  for (const [a, v0, v1] of SLITS) {
    const pane = (rad, y0, y1, col, prim) => prim.quad(pt(a - SLIT_HALF * 0.8, y0, rad), pt(a + SLIT_HALF * 0.8, y0, rad), pt(a + SLIT_HALF * 0.8, y1, rad), pt(a - SLIT_HALF * 0.8, y1, rad), col);
    pane(r - 0.012, v0 + 0.01, v1 - 0.01, P.window, prims.glow);
    for (const [yy0, yy1] of [[v0 - 0.025, v0 + 0.005], [v1 - 0.005, v1 + 0.025]]) {
      s.with(T4(...pt(a, 0, r)), () => s.with(RY(a), () => block(s, -0.06, yy0, -0.03, 0.06, yy1, 0.018, tint(STONE.tan, 1.05), { bevel: 0.008, faces: 'sewt' })));
    }
  }

  // ---- crown: two corbel rings, deck, twelve merlons, conical roof -----------------------------------------------------------------------------
  const ring = (rad, y0, y1, n, color, thick) => {
    const chord = 2 * rad * Math.sin(Math.PI / n) - 0.006;
    for (let i = 0; i < n; i++) {
      const a = ((i + 0.5) / n) * Math.PI * 2;
      s.with(T4(...pt(a, 0, rad)), () => s.with(RY(a), () => block(s, -chord / 2, y0, -thick, chord / 2, y1, 0.0, tint(color, 0.94 + rng() * 0.12), { bevel: 0.014, faces: 'stewn' })));
    }
  };
  ring(r + 0.015, BODY_Y1 - 0.0, BODY_Y1 + 0.07, 16, STONE.tan, 0.12);
  ring(r + 0.07, BODY_Y1 + 0.07, BODY_Y1 + 0.17, 16, STONE.sand, 0.14);
  const deck = BODY_Y1 + 0.17;
  const rimR = r + 0.07;
  const N = 16;
  for (let i = 0; i < N; i++) {
    const a0 = (i / N) * Math.PI * 2, a1 = ((i + 1) / N) * Math.PI * 2;
    s.tri([cx, deck, cz], pt(a0, deck, rimR), pt(a1, deck, rimR), tint(STONE.sand, 0.9)); // deck (faces up)
  }
  const M = 12;
  for (let i = 0; i < M; i++) {
    const a = ((i + 0.5) / M) * Math.PI * 2;
    const chord = 0.17;
    s.with(T4(...pt(a, 0, rimR - 0.04)), () => s.with(RY(a), () => block(s, -chord / 2, deck, -0.09, chord / 2, deck + 0.17, 0.0, tint(stoneColor(rng), 1.05), { bevel: 0.018, faces: 'nsewt' })));
  }
  // conical roof tucked inside the crenellation
  const roof = prims.roof;
  const rowsN = 7, segs = 20, baseR = r - 0.07, apexY = deck + 0.68;
  for (let row = 0; row < rowsN; row++) {
    const t0 = row / rowsN, t1 = (row + 1.15) / rowsN;
    const rad0 = baseR * (1 - t0), rad1 = baseR * Math.max(0, 1 - Math.min(1, t1));
    const y0 = deck + 0.02 + (apexY - deck - 0.02) * t0, y1 = deck + 0.02 + (apexY - deck - 0.02) * Math.min(1, t1);
    for (let i = 0; i < segs; i++) {
      const a0 = (i / segs) * Math.PI * 2, a1 = ((i + 1) / segs) * Math.PI * 2;
      const col = tint([P.blue1, P.blue2, P.blue2, P.blue3][Math.floor(rng() * 4)], 0.95 + t0 * 0.2);
      if (rad1 < 0.002) roof.tri(pt(a0, y0, rad0), pt(a1, y0, rad0), [cx, y1, cz], col);
      else roof.quad(pt(a0, y0, rad0), pt(a1, y0, rad0), pt(a1, y1, rad1), pt(a0, y1, rad1), col);
    }
  }
  // gold finial
  const g = prims.gold;
  rock(g, cx, apexY + 0.04, cz, 0.025, 0.04, 0.025, rng, P.gold, { detail: 1 });
}
