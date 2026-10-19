// Roofs: the hall's tiled gable roof, the two cross-gable dormers, the canopy walkway beside the west rampart.
import { tile, facing, beam, slab, tint, sub, add, scale, cross, norm, len, lerp3 } from '../lib/builder.mjs';
import { STONE } from '../lib/masonry.mjs';
import { H, HALL, WEST, FRONT, DORMERS, DOOR_X } from './layout.mjs';

/** Staggered rows of pillowed tiles over a parallelogram: `a` the eave corner, `u` along the eave, `v` up the slope. */
export function tileRoof(ctx, a, u, v, { rowH = 0.1, tileW = 0.15, colors, under } = {}) {
  const { prims: { roof }, rng } = ctx;
  let n = norm(cross(u, v));
  if (n[1] < 0) n = scale(n, -1);
  const rows = Math.max(1, Math.round(len(v) / rowH));
  const cols = Math.max(1, Math.round(len(u) / tileW));
  const pick = () => colors[Math.floor(rng() * colors.length)];
  // underlay, so the gaps between tiles read dark blue rather than see-through
  const off = scale(n, -0.004);
  facing(roof, add(a, off), add(add(a, u), off), add(add(add(a, u), v), off), add(add(a, v), off), under, n);
  for (let r = 0; r < rows; r++) {
    const dv = scale(v, r / rows), tv = scale(v, (1.16) / rows);
    const shift = r % 2 ? 0.5 : 0;
    for (let c = -1; c < cols; c++) {
      const t0 = Math.max(0, (c + shift) / cols), t1 = Math.min(1, (c + 1 + shift) / cols);
      if (t1 - t0 < 0.2 / cols) continue;
      const top = r === rows - 1;
      const vv = top ? scale(v, 1 / rows) : tv;
      const col = tint(pick(), 0.92 + (r / rows) * 0.16 + (rng() - 0.5) * 0.06);
      tile(roof, add(add(a, dv), scale(u, t0)), scale(u, t1 - t0), vv, col, n, 0.007, 0.012 + r * 0.0025);
    }
  }
}

const triOut = (prim, a, b, c, color, towards) => {
  const n = cross(sub(b, a), sub(c, a));
  if (n[0] * towards[0] + n[1] * towards[1] + n[2] * towards[2] >= 0) prim.tri(a, b, c, color);
  else prim.tri(a, c, b, color);
};

export function buildRoofs(ctx) {
  const { prims, P, rng } = ctx;
  const stone = prims.stone;
  const blues = [P.blue1, P.blue2, P.blue2, P.blue3];
  const under = P.blue4;

  // ---- hall roof ------------------------------------------------------------------------------------------------------------------------------
  const rx0 = HALL.x0 - 0.07, rx1 = HALL.x1 + 0.07;
  const zb = HALL.z0 - 0.1, zf = HALL.z1 + 0.1, zr = (HALL.z0 + HALL.z1) / 2;
  const k = (H.ridge - H.eave) / (HALL.z1 - zr);       // rise per unit of depth
  const yEave = H.eave - 0.1 * k, yR = H.ridge;
  tileRoof(ctx, [rx0, yEave, zf], [rx1 - rx0, 0, 0], [0, yR - yEave, zr - zf], { colors: blues, under });
  tileRoof(ctx, [rx1, yEave, zb], [rx0 - rx1, 0, 0], [0, yR - yEave, zr - zb], { colors: blues, under });
  // ridge cap, and the tan fascia boards along both eaves
  beam(stone, [rx0 - 0.02, yR + 0.012, zr], [rx1 + 0.02, yR + 0.012, zr], 0.07, 0.05, tint(STONE.tan, 1.05));
  beam(stone, [rx0, yEave - 0.01, zf], [rx1, yEave - 0.01, zf], 0.03, 0.05, tint(STONE.tan, 0.9));
  beam(stone, [rx0, yEave - 0.01, zb], [rx1, yEave - 0.01, zb], 0.03, 0.05, tint(STONE.tan, 0.9));
  // gable ends: stone infill and tan barge boards along the rakes
  for (const [x, nx] of [[HALL.x0, -1], [HALL.x1, 1]]) {
    const c = tint(STONE.tan, 0.95);
    triOut(stone, [x, H.eave - 0.02, HALL.z0], [x, H.eave - 0.02, HALL.z1], [x, yR, zr], c, [nx, 0, 0]);
    const rx = nx < 0 ? rx0 : rx1;
    beam(stone, [rx, yEave, zf], [rx, yR + 0.012, zr], 0.04, 0.05, tint(STONE.sand, 0.95), [1, 0, 0]);
    beam(stone, [rx, yEave, zb], [rx, yR + 0.012, zr], 0.04, 0.05, tint(STONE.sand, 0.95), [1, 0, 0]);
  }

  // ---- dormers: steep cross gables rising from the front wall -----------------------------------------------------------------------------------
  const yBase = H.eave - 0.05, yd = H.eave + 0.02, yr = 1.5, zFace = HALL.z1 + 0.025, zBack = 0.5, hw = 0.19;
  for (const xd of DORMERS) {
    // pediment
    triOut(stone, [xd - hw, yBase, zFace], [xd + hw, yBase, zFace], [xd, yr - 0.03, zFace], tint(STONE.sand, 0.96), [0, 0, 1]);
    for (const s of [-1, 1]) {
      // the two roof slopes
      const eaveX = xd + s * (hw + 0.025);
      const a = [eaveX, yd, zFace + 0.04];
      const u = [0, 0, zBack - (zFace + 0.04)];
      const v = [-s * (hw + 0.025), yr - yd + 0.02, 0];
      tileRoof(ctx, a, u, v, { colors: blues, under, rowH: 0.09, tileW: 0.12 });
      // tan barge board along the rake
      beam(stone, [eaveX, yd - 0.01, zFace + 0.045], [xd, yr + 0.02, zFace + 0.045], 0.035, 0.05, tint(STONE.sand, 1.0), [0, 0, 1]);
    }
    beam(stone, [xd, yr + 0.03, zFace + 0.04], [xd, yr + 0.03, zBack], 0.05, 0.045, tint(STONE.tan, 1.05));
  }

  // ---- canopy walkway beside the west rampart ---------------------------------------------------------------------------------------------------
  const wood = prims.wood;
  const cz0 = WEST.z0 + 0.12, cz1 = WEST.z1 - 0.1;
  const yOut = 1.0, yIn = 1.24, xOut = -0.04, xIn = WEST.x1 + 0.02;
  tileRoof(ctx, [xOut, yOut, cz0], [0, 0, cz1 - cz0], [xIn - xOut, yIn - yOut, 0], { colors: [P.blue1, P.blue2, P.blue1], under: P.blue4 });
  const post = tint(P.skyBlue, 0.95);
  const top = H.parapet - 0.08;
  for (let i = 0; i <= 3; i++) {
    const z = cz0 + 0.06 + (i * (cz1 - cz0 - 0.12)) / 3;
    slab(wood, 0.1, top, z - 0.02, 0.14, yOut - 0.02, z + 0.02, post);
    slab(wood, 0.1, yOut - 0.035, z - 0.025, 0.34, yOut - 0.005, z + 0.025, tint(P.woodLight, 0.9)); // brace beam under the slope
  }
  beam(wood, [0.12, yOut - 0.02, cz0], [0.12, yOut - 0.02, cz1], 0.04, 0.04, tint(P.woodLight, 0.95));

  // ---- small pent roof over the gate ---------------------------------------------------------------------------------------------------------
  const gw = 0.27;
  tileRoof(ctx, [DOOR_X - gw, 0.56, FRONT.z1 + 0.2], [2 * gw, 0, 0], [0, 0.16, -0.2], { colors: [P.blue1, P.blue2, P.blue2], under: P.blue4, rowH: 0.08, tileW: 0.11 });
  for (const sd of [-1, 1]) beam(prims.wood, [DOOR_X + sd * (gw - 0.02), 0.56, FRONT.z1 + 0.18], [DOOR_X + sd * (gw - 0.02), 0.5, FRONT.z1 + 0.18], 0.03, 0.03, tint(P.skyBlue, 0.9));
}
