// Dressing a tile grid with kit pieces (pure data, no three.js): which piece goes where, from the grid, its edge classes and a seed.
// Output: { [pieceName]: [{ x, y, z, ry, s, sy, tilt }] } in metres; the renderer turns every entry into one instance of that piece.
//   x, z      position on the ground (pieces sit on the tile surface: y = tileHeightAt, never snapped to the 0.2 m grid)
//   ry        rotation about +Y; a north-facing piece turned by ry faces (-sin ry, -cos ry), the FACING_ROTATION convention
//   s, sy     uniform scale, and an extra vertical scale (cliff faces and banks are one unit tall: sy = the step they cover)
//   tilt      optional [nx, ny, nz] ground normal to lean the piece onto (rocks, stones)
//
// Edge pieces (Cliff_*, Bank_*, Lip_Grass, Cliff_Corner) follow classifyEdges(); scatter pieces (trees, bushes, rocks, grass) follow
// broad noise fields with deterministic per-tile hashes, so the same grid and seed always dress the same way.
import { GROUND, NO_WATER, STEP } from './tile_grid.js';
import { tileHeightAt, tileNormalAt } from './tile_shapes.js';
import { classifyEdges } from './tile_edges.js';
import { hash2, mulberry32 } from '../math/rng.js';
import { fbm, smoothstep } from '../math/noise.js';

/** Rotation that turns a north-facing (-Z) piece to look along (nx, nz). */
export const facingRotation = (nx, nz) => Math.atan2(-nx, -nz);

const CLIFFS = ['Cliff_A', 'Cliff_B', 'Cliff_C'];
const LOW_CLIFFS = ['Cliff_Low', 'Cliff_Low_B'];   // boulder steps for short cliffs
const BANKS = ['Bank_A', 'Bank_B'];

/**
 * @param {import('./tile_grid.js').TileGrid} grid
 * @param {{ seed?: number, region?: string[], exclude?: Array<{x0,z0,x1,z1}>, ramps?: Array, path?: number[][], distToPath?: (x,z)=>number }} opts
 */
export function dressGrid(grid, opts = {}) {
  const seed = opts.seed ?? 3;
  const out = {};
  const put = (name, e) => (out[name] ??= []).push({ s: 1, sy: 1, ry: 0, ...e });
  const rnd = mulberry32(seed * 7919 + 1);
  const H = (tx, tz, k = 0) => hash2(tx * 13 + k * 1031, tz * 17 - k * 977, seed);
  const edges = classifyEdges(grid, { border: true, skirtBase: opts.skirtBase ?? -3 });
  const idx = (tx, tz) => grid.index(tx, tz);
  const isWater = (tx, tz) => grid.inBounds(tx, tz) && grid.water[idx(tx, tz)] !== NO_WATER;
  const ground = (tx, tz) => grid.ground[idx(tx, tz)];
  const excluded = (x, z, m = 0) => (opts.exclude ?? []).some((r) => x > r.x0 - m && x < r.x1 + m && z > r.z0 - m && z < r.z1 + m);

  // ---- edges: cliff faces, banks, grass lips, corner posts, rubble, waterfalls ------------------------------------------------------------------
  const cliffTop = new Uint8Array(grid.tileCount), cliffFoot = new Uint8Array(grid.tileCount);
  const corners = new Map();   // "x,z" -> cliff edges ending there
  const falls = [];
  for (const e of edges) {
    const ry = facingRotation(e.nx, e.nz);
    const h = e.high - e.low;
    const hiT = e.highTile, loT = e.lowTile;
    const hiGrass = [GROUND.grass, GROUND.pad, GROUND.sand].includes(ground(hiT.tx, hiT.tz)) && !isWater(hiT.tx, hiT.tz);
    const k = H(Math.round(e.x * 2), Math.round(e.z * 2), 1);
    if (e.kind === 'border') {   // a rocky rim along the top of the island's side; the soil wall continues below it
      if (isWater(hiT.tx, hiT.tz)) continue;
      const rim = 1.3 + H(e.x * 3, e.z * 3, 11) * 0.9;
      put(CLIFFS[Math.floor(k * 3)], { x: e.x, y: e.high - rim, z: e.z, ry, sy: rim + 0.04, sx: 0.98 + k * 0.12 });
      if (hiGrass) put('Lip_Grass', { x: e.x, y: e.high, z: e.z, ry, sy: 0.8 });
      continue;
    }
    const fall = isWater(hiT.tx, hiT.tz) && isWater(loT.tx, loT.tz) && h > 0.5;
    if (fall) { falls.push({ ...e, top: grid.waterHeight(hiT.tx, hiT.tz), bottom: grid.waterHeight(loT.tx, loT.tz) }); continue; }   // the water sheet dresses it
    if (e.kind === 'cliff') {
      cliffTop[idx(hiT.tx, hiT.tz)] = 1;
      cliffFoot[idx(loT.tx, loT.tz)] = 1;
      const low = h <= 1.25 && H(e.x * 7, e.z * 7, 10) < 0.75;
      put(low ? LOW_CLIFFS[Math.floor(k * 2)] : CLIFFS[Math.floor(k * 3)], { x: e.x, y: e.low - 0.06, z: e.z, ry: ry + (H(e.x * 3, e.z * 3, 2) - 0.5) * 0.08, s: 1, sy: h + 0.08, sx: 0.98 + k * 0.12 });
      if (hiGrass) put('Lip_Grass', { x: e.x, y: e.high, z: e.z, ry, sy: Math.min(1.2, 0.6 + h * 0.25) });
      for (const [px, pz] of [[e.ax, e.az], [e.bx, e.bz]]) {
        const key = `${px},${pz}`;
        if (!corners.has(key)) corners.set(key, []);
        corners.get(key).push(e);
      }
      if (!isWater(loT.tx, loT.tz) && H(e.x * 5, e.z * 5, 3) < 0.28) {   // rubble at the foot
        const off = 0.35 + H(e.x, e.z, 4) * 0.35, lat = (H(e.x, e.z, 5) - 0.5) * 0.8;
        const x = e.x + e.nx * off + e.nz * lat, z = e.z + e.nz * off + e.nx * lat;
        const big = H(e.x, e.z, 6) < 0.35 && h > 1.2;
        put(big ? 'Rock_Medium' : 'Rock_Pebbles', { x, y: tileHeightAt(grid, x, z), z, ry: H(e.x, e.z, 7) * 6.28, s: 0.7 + H(e.x, e.z, 8) * 0.6, tilt: tileNormalAt(grid, x, z) });
      }
    } else if (h >= 0.12) {
      put(BANKS[Math.floor(k * 2)], { x: e.x, y: e.low - 0.04, z: e.z, ry, sy: h + 0.06 });
      if (hiGrass) put('Lip_Grass', { x: e.x, y: e.high, z: e.z, ry, sy: 0.45 });
    }
  }
  // corner posts where cliffs facing different ways meet (outer and inner corners)
  for (const [key, list] of corners) {
    const dirs = new Set(list.map((e) => `${e.nx},${e.nz}`));
    if (dirs.size < 2) continue;
    const [x, z] = key.split(',').map(Number);
    const lo = Math.min(...list.map((e) => e.low)), hi = Math.max(...list.map((e) => e.high));
    put('Cliff_Corner', { x, y: lo - 0.06, z, ry: H(x, z, 9) * 6.28, sy: hi - lo + 0.06 });
  }

  // ---- scatter -------------------------------------------------------------------------------------------------------------------------------------
  const taken = [];   // [x, z, r] trunks and boulders, for spacing
  const free = (x, z, r) => taken.every(([a, b, q]) => Math.hypot(x - a, z - b) > r + q);
  const region = opts.region ?? [];
  const dPath = opts.distToPath ?? (() => 99);
  const nearWater = (tx, tz, r = 1) => { for (let dz = -r; dz <= r; dz++) for (let dx = -r; dx <= r; dx++) if (isWater(tx + dx, tz + dz)) return true; return false; };

  for (let tz = 0; tz < grid.depth; tz++) for (let tx = 0; tx < grid.width; tx++) {
    const i = idx(tx, tz), g = grid.ground[i], reg = region[i] ?? '';
    if (isWater(tx, tz)) {
      if (H(tx, tz, 20) < 0.08) { const x = tx + 0.2 + H(tx, tz, 21) * 0.6, z = tz + 0.2 + H(tx, tz, 22) * 0.6; put('Rock_Pebbles', { x, y: tileHeightAt(grid, x, z) - 0.02, z, ry: H(tx, tz, 23) * 6.28, s: 1.2, tilt: tileNormalAt(grid, x, z) }); }
      continue;
    }
    if (g === GROUND.pad || reg === 'ramp') continue;
    const cx = tx + 0.5, cz = tz + 0.5;
    if (excluded(cx, cz, 0.6)) continue;
    const dp = dPath(cx, cz);
    const forest = fbm(cx * 0.085, cz * 0.085, seed + 40, 3);
    const flowerField = fbm(cx * 0.14, cz * 0.14, seed + 50, 2);
    const jx = () => tx + 0.12 + rnd() * 0.76, jz = () => tz + 0.12 + rnd() * 0.76;
    const onPath = g === GROUND.path;

    if (!onPath) {
      // trees: forest clumps, kept off the path, the water's edge and cliff lips
      const conifer = ['west', 'ridge', 'riseB', 'plateau'].includes(reg);
      const dens = 0.012 + 0.2 * smoothstep(0.5, 0.68, forest);
      if (dp > 2.4 && !nearWater(tx, tz) && H(tx, tz, 30) < dens) {
        const x = jx(), z = jz();
        const r = conifer ? 1.1 : 1.7;
        if (free(x, z, r) && !(cliffTop[i] && H(tx, tz, 31) < 0.7) && !excluded(x, z, 3)) {
          const pick = H(tx, tz, 32);
          const name = conifer ? (pick < 0.75 ? 'Tree_Conifer' : 'Tree_Round')
            : pick < 0.45 ? 'Tree_Broadleaf' : pick < 0.8 ? 'Tree_Round' : pick < 0.92 ? 'Tree_Blossom' : 'Tree_Conifer';
          put(name, { x, y: tileHeightAt(grid, x, z) - 0.05, z, ry: rnd() * 6.28, s: 0.8 + rnd() * 0.4 });
          taken.push([x, z, r]);
          if (rnd() < 0.6) { const a = rnd() * 6.28, bx = x + Math.cos(a) * 1.1, bz = z + Math.sin(a) * 1.1; if (grid.tileAt(bx, bz)) put(rnd() < 0.5 ? 'Bush_Fern' : 'Bush_Dark', { x: bx, y: tileHeightAt(grid, bx, bz), z: bz, ry: rnd() * 6.28, s: 0.8 + rnd() * 0.5 }); }
          continue;
        }
      }
      // boulders, rarely, in the open
      if (H(tx, tz, 33) < 0.012 && dp > 1.8) {
        const x = jx(), z = jz();
        if (free(x, z, 1)) { put('Rock_Boulder', { x, y: tileHeightAt(grid, x, z), z, ry: rnd() * 6.28, s: 0.6 + rnd() * 0.6, tilt: tileNormalAt(grid, x, z) }); taken.push([x, z, 1]); }
      }
      // bushes: along cliff tops and the path's edges, a few elsewhere
      const pb = cliffTop[i] ? 0.22 : dp < 2.4 ? 0.16 : cliffFoot[i] ? 0.12 : 0.035;
      if (H(tx, tz, 34) < pb) {
        const x = jx(), z = jz();
        if (free(x, z, 0.5)) {
          const pick = H(tx, tz, 35);
          put(pick < 0.45 ? 'Bush_Round' : pick < 0.75 ? 'Bush_Dark' : pick < 0.88 ? 'Bush_Blossom' : 'Bush_Fern', { x, y: tileHeightAt(grid, x, z) - 0.03, z, ry: rnd() * 6.28, s: 0.75 + rnd() * 0.6 });
          taken.push([x, z, 0.45]);
        }
      }
      if (g === GROUND.sand && H(tx, tz, 36) < 0.55) { const x = jx(), z = jz(); put('Reeds', { x, y: tileHeightAt(grid, x, z), z, ry: rnd() * 6.28, s: 0.7 + rnd() * 0.5 }); }
    }
    // grass tufts and flowers everywhere green (sparse on the path, none in its middle)
    const tufts = onPath ? (dp > 0.8 && H(tx, tz, 37) < 0.35 ? 1 : 0) : 2 + (H(tx, tz, 38) < 0.5 ? 1 : 0);
    for (let k = 0; k < tufts; k++) {
      const x = jx(), z = jz();
      const flowers = !onPath && flowerField > 0.6 && rnd() < 0.55;
      const name = flowers ? 'Flowers' : !onPath && rnd() < 0.18 ? 'Grass_Tall' : 'Grass_Tuft';
      put(name, { x, y: tileHeightAt(grid, x, z) - 0.01, z, ry: rnd() * 6.28, s: 0.7 + rnd() * 0.6 });
    }
  }

  // ---- path steps on the ramps (logs across the corridor every metre) -------------------------------------------------------------------------------
  for (const r of opts.ramps ?? []) {
    const alongZ = r.axis === 'z';
    const down = alongZ ? (r.highAtStart ? [0, 1] : [0, -1]) : (r.highAtStart ? [1, 0] : [-1, 0]);   // the downhill direction
    const ry = facingRotation(down[0], down[1]);
    const width = alongZ ? r.x1 - r.x0 : r.z1 - r.z0;
    const n = alongZ ? r.z1 - r.z0 : r.x1 - r.x0;
    for (let k = 1; k < n; k++) {
      const x = alongZ ? (r.x0 + r.x1) / 2 : r.x0 + k, z = alongZ ? r.z0 + k : (r.z0 + r.z1) / 2;
      put('Path_Step', { x, y: tileHeightAt(grid, x, z), z, ry, sx: width * 0.86 });
    }
  }

  return { pieces: out, edges, falls };
}

export { STEP };
