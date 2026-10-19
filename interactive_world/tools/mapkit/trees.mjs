// Trees and bushes. Each builder adds geometry to a prim set (see palette.mjs) at (x, y, z) with a height `h`; `rng` makes each one unique.
import { tint } from '../lib/builder.mjs';
import { cylinder, cone, blob } from '../lib/solids.mjs';
import { C } from './palette.mjs';

/** Conifer: a slim trunk and 4 stacked cones that shrink towards the top. Forest pines use assets/pine.glb instead; this one still decorates other pieces (outcrops, towers). */
export function pine(p, rng, { x = 0, y = 0, z = 0, h = 1.8, tiers = 4 } = {}) {
  const trunkH = h * 0.22, r = h * 0.3;
  cylinder(p.wood, x, y, z, h * 0.05, h * 0.04, trunkH + h * 0.1, 6, C.trunk);
  const greens = [C.pine1, C.pine2, C.pine3];
  const coneH = (h - trunkH) / (tiers * 0.62 + 0.38);
  for (let i = 0; i < tiers; i++) {
    const base = greens[Math.floor(rng() * greens.length)];
    const rr = r * (1 - i * (0.8 / tiers));
    cone(p.leaf, x + (rng() - 0.5) * 0.02 * h, y + trunkH + i * coneH * 0.62, z + (rng() - 0.5) * 0.02 * h, rr, coneH, 7, tint(base, 0.9 + rng() * 0.2), { spin: rng() * 6 });
  }
}

/** Broadleaf tree: a trunk and a crown of puffy blobs in two or three greens. */
export function oak(p, rng, { x = 0, y = 0, z = 0, h = 1.5, blobs = 5 } = {}) {
  const trunkH = h * 0.4;
  cylinder(p.wood, x, y, z, h * 0.075, h * 0.05, trunkH, 6, C.trunk);
  const greens = [C.leaf1, C.leaf2, C.leaf3];
  const R = h * 0.3;
  blob(p.leaf, x, y + trunkH + R * 0.55, z, R * 1.02, R * 0.75, R * 1.02, tint(C.leafDark, 0.95), rng);   // dark underside
  for (let i = 0; i < blobs; i++) {
    const a = (i / blobs) * Math.PI * 2 + rng(), d = i === 0 ? 0 : R * (0.45 + rng() * 0.3);
    const up = i === 0 ? R * 1.25 : R * (0.7 + rng() * 0.5);
    const rr = R * (i === 0 ? 0.95 : 0.62 + rng() * 0.25);
    blob(p.leaf, x + Math.sin(a) * d, y + trunkH + up, z + Math.cos(a) * d, rr, rr * 0.85, rr, tint(greens[Math.floor(rng() * greens.length)], 0.92 + rng() * 0.16), rng);
  }
}

/** Low bush: two or three small blobs on the ground. */
export function bush(p, rng, { x = 0, y = 0, z = 0, s = 0.3 } = {}) {
  for (let i = 0; i < 3; i++) {
    blob(p.leaf, x + (rng() - 0.5) * s, y + s * 0.38, z + (rng() - 0.5) * s, s * (0.55 + rng() * 0.2), s * 0.45, s * (0.55 + rng() * 0.2), tint([C.leaf1, C.leaf2, C.leaf3][i], 0.95 + rng() * 0.1), rng, 0);
  }
}
