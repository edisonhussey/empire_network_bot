// Wooden bridges and a dock. A bridge runs along z (so rotate it to follow the road), `len` tiles long and centred on the origin; its deck
// arches gently and sits above the water. Piers go down to y = -0.5.
import { block, beam, tint } from '../lib/builder.mjs';
import { cylinder } from '../lib/solids.mjs';
import { C } from './palette.mjs';
import { T } from './nature.mjs';

// The model's own deck profile, in modelled units (the map scales the piece at placement; the field's BRIDGE in constants.js is that profile in metres).
export const DECK_Y = 0.14;   // deck height at the ends; the middle arches `ARCH` above it
export const ARCH = 0.12;

export function bridge(p, rng, { len = 4, width = 0.95 } = {}) {
  const L = len * T, W = width * T, hw = W / 2;
  const y = (z) => DECK_Y + ARCH * Math.sin(Math.PI * (z / L + 0.5));
  const n = Math.max(6, Math.round(L / (0.17 * T)));
  for (let i = 0; i < n; i++) {                                                          // deck planks
    const z0 = -L / 2 + (L * i) / n, z1 = -L / 2 + (L * (i + 1)) / n - 0.012, col = tint(C.plank, 0.86 + (i % 3) * 0.1 + rng() * 0.05), t = 0.08;
    const top0 = y(z0), top1 = y(z1);
    p.wood.quad([-hw, top0, z0], [-hw, top1, z1], [hw, top1, z1], [hw, top0, z0], col);   // top
    p.wood.quad([hw, top0, z0], [hw, top1, z1], [hw, top1 - t, z1], [hw, top0 - t, z0], tint(col, 0.8));
    p.wood.quad([-hw, top0 - t, z0], [-hw, top1 - t, z1], [-hw, top1, z1], [-hw, top0, z0], tint(col, 0.8));
    p.wood.quad([-hw, top1, z1], [hw, top1, z1], [hw, top1 - t, z1], [-hw, top1 - t, z1], tint(col, 0.7));
  }
  for (const sx of [-1, 1]) {                                                            // side beams, posts and rails
    for (let i = 0; i < n; i++) {
      const z0 = -L / 2 + (L * i) / n, z1 = -L / 2 + (L * (i + 1)) / n;
      beam(p.wood, [sx * (hw + 0.02), y(z0) - 0.1, z0], [sx * (hw + 0.02), y(z1) - 0.1, z1], 0.06, 0.1, C.woodDark, [0, 1, 0]);
    }
    const posts = Math.max(3, Math.round(L / (0.5 * T)));
    let prev = null;
    for (let i = 0; i <= posts; i++) {
      const z = -L / 2 + (L * i) / posts, ph = i === 0 || i === posts ? 0.52 : 0.4;
      const x = sx * (hw + 0.02);
      block(p.wood, x - 0.035, y(z) - 0.02, z - 0.035, x + 0.035, y(z) + ph, z + 0.035, tint(C.woodDark, 1.0 + rng() * 0.15), { bevel: 0.006 });
      if (prev) {
        beam(p.wood, [x, prev.y + 0.3, prev.z], [x, y(z) + 0.3, z], 0.035, 0.05, C.wood, [0, 1, 0]);
        beam(p.wood, [x, prev.y + 0.16, prev.z], [x, y(z) + 0.16, z], 0.03, 0.04, C.wood, [0, 1, 0]);
      }
      prev = { y: y(z), z };
    }
  }
  for (const z of [-L * 0.4, 0, L * 0.4]) for (const sx of [-1, 1]) cylinder(p.wood, sx * hw * 0.9, -0.5, z, 0.07, 0.065, y(z) - 0.1 + 0.5, 6, C.woodDark);
}

/** A wooden dock jutting along z: planks on piles, a couple of posts. */
export function dock(p, rng, { len = 2 } = {}) {
  const L = len * T, W = 0.85 * T;
  for (let i = 0; i < 9; i++) {
    const z0 = -L / 2 + (L * i) / 9;
    block(p.wood, -W / 2, 0.1, z0, W / 2, 0.17, z0 + L / 9 - 0.015, tint(C.plank, 0.85 + (i % 3) * 0.1), { bevel: 0.006 });
  }
  for (const z of [-L * 0.42, 0, L * 0.42]) for (const sx of [-1, 1]) cylinder(p.wood, sx * W * 0.45, -0.45, z, 0.06, 0.055, 0.62, 6, C.woodDark);
  for (const sx of [-1, 1]) block(p.wood, sx * W * 0.5 - 0.035, 0.17, -L / 2, sx * W * 0.5 + 0.035, 0.5, -L / 2 + 0.07, C.woodDark, { bevel: 0.006 });
}
