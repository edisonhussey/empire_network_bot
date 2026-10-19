// A roof slope as rows of pillowed tiles or thatch bundles (no dependency on the castle's context). a = eave corner, u = along the eave, v = up the slope.
import { tile, facing, add, scale, cross, norm, len, tint } from '../lib/builder.mjs';

export function tileGrid(prim, rng, a, u, v, { rowH = 0.1, tileW = 0.14, colors, under, bevel = 0.008, lift = 0.014, stagger = true } = {}) {
  let n = norm(cross(u, v));
  if (n[1] < 0) n = scale(n, -1);
  const rows = Math.max(1, Math.round(len(v) / rowH)), cols = Math.max(1, Math.round(len(u) / tileW));
  if (under) {
    const off = scale(n, -0.004);
    facing(prim, add(a, off), add(add(a, u), off), add(add(add(a, u), v), off), add(add(a, v), off), under, n);
  }
  for (let r = 0; r < rows; r++) {
    const shift = stagger && r % 2 ? 0.5 : 0;
    for (let c = -1; c < cols; c++) {
      const t0 = Math.max(0, (c + shift) / cols), t1 = Math.min(1, (c + 1 + shift) / cols);
      if (t1 - t0 < 0.2 / cols) continue;
      const vv = r === rows - 1 ? scale(v, 1 / rows) : scale(v, 1.16 / rows);
      const col = tint(colors[Math.floor(rng() * colors.length)], 0.92 + (r / rows) * 0.14 + (rng() - 0.5) * 0.06);
      tile(prim, add(add(a, scale(v, r / rows)), scale(u, t0)), scale(u, t1 - t0), vv, col, n, bevel, lift + r * 0.002);
    }
  }
}
