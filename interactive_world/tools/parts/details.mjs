// Small things that sell the model: window frames and glowing panes, the blue doors with gold rings, steps, round gable windows, the banner.
import { slab, block, tint, T4, RY, mul4, rock } from '../lib/builder.mjs';
import { STONE } from '../lib/masonry.mjs';
import { H, HALL, KEEP, FRONT, DORMERS } from './layout.mjs';
import { TEAM_GREY } from '../../src/world/team_slots.js';
import { OPENINGS } from './stone.mjs';

/**
 * Run `fn` in a frame sitting on a wall face: origin at the face plane under the opening's centre, +z out of the wall, x along the face.
 * The transform is applied to every primitive (stone, wood, glow, gold...) so a window can use several materials.
 */
const onFace = (ctx, x, z, facingAngle, fn) => {
  const m = mul4(T4(x, 0, z), RY(facingAngle));
  const run = (list) => (list.length ? list[0].with(m, () => run(list.slice(1))) : fn());
  run(Object.values(ctx.prims));
};

function windowAt(ctx, o, place) {
  const { prims, P, rng } = ctx;
  const s = prims.stone, g = prims.glow, w = prims.wood;
  const { w: ww, y0, y1 } = o;
  const hw = ww / 2;
  const frame = tint(STONE.sand, 0.95 + rng() * 0.08);
  place(() => {
    // pane, set back in the opening, with a wooden cross
    g.quad([-hw, y0, -0.016], [hw, y0, -0.016], [hw, y1, -0.016], [-hw, y1, -0.016], P.window);
    slab(w, -0.006, y0, -0.019, 0.006, y1, -0.012, P.wood);
    slab(w, -hw, (y0 + y1) / 2 - 0.006, -0.019, hw, (y0 + y1) / 2 + 0.006, -0.012, P.wood);
    // frame: jambs, lintel and a projecting sill
    block(s, -hw - 0.026, y0 - 0.02, -0.012, -hw + 0.002, y1 + 0.02, 0.02, frame, { bevel: 0.007, faces: 'sewt' });
    block(s, hw - 0.002, y0 - 0.02, -0.012, hw + 0.026, y1 + 0.02, 0.02, frame, { bevel: 0.007, faces: 'sewt' });
    block(s, -hw - 0.034, y1 - 0.004, -0.012, hw + 0.034, y1 + 0.036, 0.026, tint(frame, 1.05), { bevel: 0.008, faces: 'sewt' });
    block(s, -hw - 0.04, y0 - 0.04, -0.012, hw + 0.04, y0 - 0.004, 0.034, tint(frame, 0.92), { bevel: 0.008, faces: 'sewt' });
  });
}

/** Blue double door with plank lines, a gold ring on each leaf, a tan frame and a timber lintel. */
function doorAt(ctx, o, place, { arch = false } = {}) {
  const { prims, P, rng } = ctx;
  const s = prims.stone, w = prims.wood, g = prims.gold;
  const hw = o.w / 2, y0 = o.y0, y1 = o.y1;
  place(() => {
    const leafTop = y1 - 0.02;
    for (const sd of [-1, 1]) {
      const x0 = sd < 0 ? -hw + 0.004 : 0.004, x1 = sd < 0 ? -0.004 : hw - 0.004;
      slab(w, x0, y0, -0.03, x1, leafTop, -0.012, tint(P.doorBlue, sd < 0 ? 0.95 : 1.05), 's');
      // planks
      const n = 3;
      for (let i = 1; i < n; i++) {
        const px = x0 + ((x1 - x0) * i) / n;
        slab(w, px - 0.003, y0, -0.014, px + 0.003, leafTop, -0.008, tint(P.doorBlue, 0.65), 's');
      }
      // iron straps and a gold ring
      for (const fy of [0.28, 0.72]) slab(w, x0, y0 + (leafTop - y0) * fy - 0.008, -0.014, x1, y0 + (leafTop - y0) * fy + 0.008, -0.008, tint(P.wood, 0.9), 's');
      const rx = sd < 0 ? -0.025 : 0.025, ry = y0 + (leafTop - y0) * 0.5;
      for (let i = 0; i < 8; i++) {
        const a0 = (i / 8) * Math.PI * 2, a1 = ((i + 1) / 8) * Math.PI * 2;
        const R = 0.02, r2 = 0.013;
        g.quad([rx + Math.sin(a0) * r2, ry + Math.cos(a0) * r2, -0.004], [rx + Math.sin(a1) * r2, ry + Math.cos(a1) * r2, -0.004], [rx + Math.sin(a1) * R, ry + Math.cos(a1) * R, -0.004], [rx + Math.sin(a0) * R, ry + Math.cos(a0) * R, -0.004], P.gold);
      }
    }
    const frame = tint(STONE.sand, 0.9);
    block(s, -hw - 0.04, y0, -0.012, -hw + 0.004, y1 + 0.02, 0.026, frame, { bevel: 0.009, faces: 'sewt' });
    block(s, hw - 0.004, y0, -0.012, hw + 0.04, y1 + 0.02, 0.026, frame, { bevel: 0.009, faces: 'sewt' });
    block(s, -hw - 0.05, y1 - 0.01, -0.012, hw + 0.05, y1 + 0.05, 0.034, tint(frame, 1.06), { bevel: 0.01, faces: 'sewt' });
    slab(w, -hw - 0.02, y1 - 0.02, -0.03, hw + 0.02, y1 - 0.004, 0.02, P.woodLight);
  });
}

function roundWindow(ctx, x, y, z, facingAngle, rad) {
  const { prims, P } = ctx;
  const s = prims.stone, g = prims.glow;
  onFace(ctx, x, z, facingAngle, () => {
    const n = 12;
    for (let i = 0; i < n; i++) {
      const a0 = (i / n) * Math.PI * 2, a1 = ((i + 1) / n) * Math.PI * 2;
      g.tri([0, y, 0.004], [Math.sin(a0) * rad, y + Math.cos(a0) * rad, 0.004], [Math.sin(a1) * rad, y + Math.cos(a1) * rad, 0.004], P.window);
      // tan ring
      s.quad([Math.sin(a0) * rad, y + Math.cos(a0) * rad, 0.004], [Math.sin(a1) * rad, y + Math.cos(a1) * rad, 0.004], [Math.sin(a1) * (rad + 0.03), y + Math.cos(a1) * (rad + 0.03), 0.014], [Math.sin(a0) * (rad + 0.03), y + Math.cos(a0) * (rad + 0.03), 0.014], tint(STONE.tan, 1.1));
    }
  });
}

export function buildDetails(ctx) {
  const { prims, P, rng } = ctx;
  const s = prims.stone, wood = prims.wood;

  // ---- windows and doors on every face, driven by the same lists the masonry used for the holes ----------------------------------------------
  for (const o of OPENINGS.hallS) (o.kind === 'door' ? doorAt : windowAt)(ctx, o, (fn) => onFace(ctx, o.c, HALL.z1, 0, fn));
  for (const o of OPENINGS.hallN) windowAt(ctx, o, (fn) => onFace(ctx, o.c, HALL.z0, Math.PI, fn));
  for (const o of OPENINGS.hallW) windowAt(ctx, o, (fn) => onFace(ctx, HALL.x0, o.c, (3 * Math.PI) / 2, fn));
  for (const o of OPENINGS.keepS) windowAt(ctx, o, (fn) => onFace(ctx, o.c, KEEP.z1, 0, fn));
  for (const o of OPENINGS.keepE) windowAt(ctx, o, (fn) => onFace(ctx, KEEP.x1, o.c, Math.PI / 2, fn));
  doorAt(ctx, OPENINGS.gate, (fn) => onFace(ctx, OPENINGS.gate.c, FRONT.z1, 0, fn));

  // round windows in the dormer pediments and the hall's west gable
  for (const xd of DORMERS) roundWindow(ctx, xd, 1.28, HALL.z1 + 0.03, 0, 0.055);
  roundWindow(ctx, HALL.x0, 1.28, 0.46, (3 * Math.PI) / 2, 0.07);

  // ---- steps up to the gate and a timber lintel beam over it ----------------------------------------------------------------------------------
  const gx = OPENINGS.gate.c;
  for (let k = 0; k < 3; k++) {
    const half = 0.2 + k * 0.03, top = 0.12 - k * 0.04, depth = 0.06 * (k + 1);
    block(s, gx - half, 0.0, FRONT.z1, gx + half, top, FRONT.z1 + depth, tint(STONE.tan, 0.85 + k * 0.07), { bevel: 0.012, faces: 'sewt' });
  }

  // ---- timber: a plank band under the hall eaves on the front, and short corbel brackets -----------------------------------------------------------
  slab(wood, 0.0, H.eave - 0.05, HALL.z1 - 0.0, HALL.x1, H.eave - 0.018, HALL.z1 + 0.03, tint(P.woodLight, 0.9), 'swt');
  for (let x = 0.12; x < 2.1; x += 0.24) slab(wood, x, H.eave - 0.14, HALL.z1, x + 0.03, H.eave - 0.05, HALL.z1 + 0.03, P.wood, 'sewt');

  // ---- banner on the keep ------------------------------------------------------------------------------------------------------------------------
  const fx = KEEP.x0 + 0.3, fz = KEEP.z0 + 0.26, yBase = H.keepTop + 0.04;
  slab(wood, fx - 0.012, yBase, fz - 0.012, fx + 0.012, yBase + 0.86, fz + 0.012, tint(P.woodLight, 0.85));
  const gold = prims.gold;
  rock(gold, fx, yBase + 0.88, fz, 0.025, 0.025, 0.025, rng, P.gold, { detail: 1 });
  const fa = prims.teamA, fb = prims.teamB, G = [TEAM_GREY, TEAM_GREY, TEAM_GREY];
  const Lf = 0.55, Hf = 0.3, seg = 10, y0 = yBase + 0.55;
  for (let i = 0; i < seg; i++) {
    const t0 = i / seg, t1 = (i + 1) / seg;
    const z = (t) => fz + 0.012 + t * Lf, wob = (t) => Math.sin(t * 5.2 + 0.4) * 0.025 * t;
    const droop = (t) => -t * 0.035;
    const a = [fx + wob(t0), y0 + droop(t0), z(t0)], b = [fx + wob(t1), y0 + droop(t1), z(t1)];
    const col = (k) => tint(k, 0.9 + 0.15 * Math.sin(i * 1.3));
    fb.quad(a, b, [b[0], b[1] + Hf * 0.5, b[2]], [a[0], a[1] + Hf * 0.5, a[2]], col(G));
    fa.quad([a[0], a[1] + Hf * 0.5, a[2]], [b[0], b[1] + Hf * 0.5, b[2]], [b[0], b[1] + Hf, b[2]], [a[0], a[1] + Hf, a[2]], col(G));
  }
}
