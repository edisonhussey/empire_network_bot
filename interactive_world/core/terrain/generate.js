// Demonstration world generator (deterministic per seed). Broad, intentional landforms come first; noise only perturbs them:
//   rolling lowland base, a terraced hill (NW), a mesa with a cliff rim and one access ramp (E), an escarpment line (W), a shallow valley with
//   gentle sides (S), four exactly flat building pads, and a patch of shallow irregular cells (SE).
// The landforms are one continuous function landform(x, z) in metres. Each cell then samples it at its centre and its four corners and keeps the
// corners within the cell's allowed spread of the centre height. That cap is what turns steeper ground into steps: a cell can tilt at most
// `spread` metres over 5 m, so faster rises are carried by small risers between cells, which the connector pass then fills.
import { TerrainGrid, CORNER_OFFSET } from './grid.js';
import { mulberry32, hash2 } from '../math/rng.js';
import { fbm, valueNoise, smoothstep, clamp, lerp } from '../math/noise.js';

/** Zone of a cell, kept beside the heights for later placement rules and the debug view. */
export const ZONE = { rolling: 0, pad: 1, terrace: 2, plateau: 3, slope: 4, cliffband: 5, irregular: 6 };
export const ZONE_NAMES = Object.keys(ZONE);

// Allowed corner spread per zone (metres). The spec's usual limit is 0.2 m; built slopes (ramp, valley sides) may tilt more so they stay one
// continuous ramp instead of stairs, and pads are exactly flat.
const SPREAD = { rolling: 0.2, pad: 0, terrace: 0.05, plateau: 0.2, slope: 1.2, cliffband: 0.2, irregular: 0.2 };

/**
 * Generate the demonstration world. Returns { grid, zones (Uint8Array, one per cell), features (named landforms, world metres), stats }.
 * @param {{ seed?: number, width?: number, depth?: number, cellSize?: number }} opts
 */
export function generateWorld({ seed = 7, width = 100, depth = 100, cellSize = 5 } = {}) {
  const grid = new TerrainGrid({ width, depth, cellSize });
  const W = width * cellSize, D = depth * cellSize;
  const rng = mulberry32(seed * 9973 + 17);
  const J = (m) => (rng() - 0.5) * 2 * m;   // seeded jitter of feature placement (metres)

  // ---- landforms, in world metres (laid out for a 500 x 500 m map; scaled if the map is another size) ------------------------------------
  const sx = W / 500, sz = D / 500;
  const hill = { x: (115 + J(10)) * sx, z: (120 + J(10)) * sz, r: 85 * sx, peak: 16, step: 2 };
  const mesa = { x: (355 + J(10)) * sx, z: (150 + J(10)) * sz, r: 78 * sx, top: 12 };
  const ramp = { a: [255 * sx, 248 * sz], b: [318 * sx, 196 * sz], half: 8 };
  const valley = { z: 362 * sz, amp: 22 * sz, phase: rng() * 6.28, floor: 10, side: 40, depth: 4 };
  const scarp = { z: 285 * sz, amp: 12 * sz, end: 235 * sx, rise: 2.5 };
  const pads = [
    { name: 'pad on the mesa', x0: 345 * sx, z0: 115 * sz, x1: 395 * sx, z1: 155 * sz },
    { name: 'pad in the middle', x0: 180 * sx, z0: 170 * sz, x1: 220 * sx, z1: 210 * sz },
    { name: 'pad in the east lowland', x0: 425 * sx, z0: 262 * sz, x1: 475 * sx, z1: 300 * sz },
    { name: 'pad south of the valley', x0: 60 * sx, z0: 440 * sz, x1: 120 * sx, z1: 475 * sz },
  ];
  const irregular = { x0: 380 * sx, z0: 420 * sz };

  const base = (x, z) => 2 + 3 * (fbm(x / 240, z / 240, seed, 3) - 0.5) * 2 + 0.8 * (fbm(x / 70, z / 70, seed + 7, 2) - 0.5) * 2;
  const valleyZ = (x) => valley.z + valley.amp * Math.sin(x / 80 + valley.phase);
  const scarpZ = (x) => scarp.z + scarp.amp * Math.sin(x / 60 + 1.3);

  /** Composite landform without pads: { h, zone }. */
  function natural(x, z) {
    let h = base(x, z), zone = 'rolling';
    // valley: flat floor, gentle sides
    const dv = Math.abs(z - valleyZ(x));
    if (dv < valley.side) { h -= valley.depth * (1 - smoothstep(valley.floor, valley.side, dv)); if (dv > valley.floor) zone = 'slope'; }
    // escarpment: the ground north of a wavy line stands `rise` higher; the step is ~1 cell wide, so it becomes a line of low cliffs
    const fade = 1 - smoothstep(scarp.end - 30, scarp.end + 20, x);
    if (fade > 0) {
      const sd = scarpZ(x) - z;   // > 0 north of the line
      h += scarp.rise * fade * smoothstep(-2, 2, sd);
      if (Math.abs(sd) < 6 && fade > 0.3) zone = 'cliffband';
    }
    // terraced hill: a dome cut into flat `step` m terraces (noise wobbles the terrace rims so they are not perfect circles)
    const ang = Math.atan2(z - hill.z, x - hill.x);
    const dh = Math.hypot(x - hill.x, z - hill.z) * (1 + 0.14 * (valueNoise(Math.cos(ang) * 2 + 5, Math.sin(ang) * 2 + 5, seed + 3) - 0.5) * 2);
    if (dh < hill.r) { const dome = hill.peak * (1 - (dh / hill.r) ** 2); h += Math.floor(dome / hill.step) * hill.step; if (dome >= hill.step) zone = 'terrace'; }
    // mesa: a cliff rim (the 5 m transition is one cell wide), a flattish top, and one ramp up its south-west side
    const am = Math.atan2(z - mesa.z, x - mesa.x);
    const rim = mesa.r * (1 + 0.16 * (valueNoise(Math.cos(am) * 1.7 + 9, Math.sin(am) * 1.7 + 9, seed + 5) - 0.5) * 2);
    let m = 1 - smoothstep(rim - 2.5, rim + 2.5, Math.hypot(x - mesa.x, z - mesa.z));
    const [ax, az] = ramp.a, [bx, bz] = ramp.b, rx = bx - ax, rz = bz - az, rl2 = rx * rx + rz * rz;
    const t = ((x - ax) * rx + (z - az) * rz) / rl2, off = Math.abs((x - ax) * rz - (z - az) * rx) / Math.sqrt(rl2);
    const onRamp = t > -0.05 && t < 1.05 && off < ramp.half && m < 0.999;
    if (onRamp) m = Math.max(m, clamp(t, 0, 1));
    if (m > 0) h = lerp(h, mesa.top + 0.8 * (fbm(x / 50, z / 50, seed + 11, 2) - 0.5) * 2, m);
    if (onRamp && m > 0.001) zone = 'slope';
    else if (m > 0.98) zone = 'plateau';
    // shallow irregular patch in the south-east
    if (zone === 'rolling' && x > irregular.x0 && z > irregular.z0) zone = 'irregular';
    return { h, zone };
  }

  // pads: exactly flat at the natural height of their centre, rounded to 0.5 m
  for (const p of pads) p.level = Math.round(natural((p.x0 + p.x1) / 2, (p.z0 + p.z1) / 2).h * 2) / 2;
  function landform(x, z) {
    for (const p of pads) if (x >= p.x0 && x <= p.x1 && z >= p.z0 && z <= p.z1) return { h: p.level, zone: 'pad' };
    return natural(x, z);
  }

  // ---- sample per cell: centre height, corners kept within the zone's spread ------------------------------------------------------------------
  const zones = new Uint8Array(width * depth);
  const r2 = (v) => Math.round(v * 100) / 100;   // centimetres: clean numbers like [22.0, 22.2, 22.0, 22.0]
  const surf = new Float32Array(4);
  for (let cz = 0; cz < depth; cz++) {
    for (let cx = 0; cx < width; cx++) {
      const c = landform((cx + 0.5) * cellSize, (cz + 0.5) * cellSize);
      const half = SPREAD[c.zone] / 2;
      for (let k = 0; k < 4; k++) {
        const x = (cx + CORNER_OFFSET[k][0]) * cellSize, z = (cz + CORNER_OFFSET[k][1]) * cellSize;
        let d = clamp(landform(x, z).h - c.h, -half, half);
        if (c.zone === 'irregular') d = clamp(d + (hash2(cx * 4 + k, cz, seed + 31) - 0.5) * 0.14, -half, half);
        surf[k] = r2(c.h + d);
      }
      grid.setSurface(cx, cz, surf);
      zones[grid.index(cx, cz)] = ZONE[c.zone];
    }
  }

  const features = [
    { name: 'terraced hill', kind: 'terrace', x: hill.x, z: hill.z, r: hill.r },
    { name: 'mesa', kind: 'plateau', x: mesa.x, z: mesa.z, r: mesa.r },
    { name: 'mesa ramp', kind: 'slope', x: (ramp.a[0] + ramp.b[0]) / 2, z: (ramp.a[1] + ramp.b[1]) / 2, r: 40 },
    { name: 'escarpment', kind: 'cliffband', x: scarp.end / 2, z: scarp.z, r: scarp.end / 2 },
    { name: 'valley', kind: 'slope', x: W / 2, z: valley.z, r: W / 2 },
    { name: 'irregular patch', kind: 'irregular', x: (irregular.x0 + W) / 2, z: (irregular.z0 + D) / 2, r: 60 },
    ...pads.map((p) => ({ name: p.name, kind: 'pad', x: (p.x0 + p.x1) / 2, z: (p.z0 + p.z1) / 2, r: Math.max(p.x1 - p.x0, p.z1 - p.z0) / 2, level: p.level })),
  ];
  return { grid, zones, features, stats: surfaceStats(grid) };
}

/** How many cells are flat, within 0.2 m, or steeper (for the spec's "usually <= 0.2 m" rule). */
export function surfaceStats(grid) {
  let flat = 0, within = 0, max = 0;
  for (let cz = 0; cz < grid.depth; cz++) for (let cx = 0; cx < grid.width; cx++) {
    const s = grid.spread(cx, cz);
    if (s < 0.005) flat++;
    if (s <= 0.2 + 1e-4) within++;
    if (s > max) max = s;
  }
  const n = grid.cellCount;
  return { cells: n, flat, withinSpec: within, withinSpecShare: within / n, maxSpread: Math.round(max * 100) / 100 };
}
