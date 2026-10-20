// The example tile scene (after the "Quantized Heightmap Terrain" guide sheet): a 48 x 40 m island with a north-east plateau carrying the
// shrine pad, a middle terrace, a rocky stepped rise in the east, a west upland, a stream that falls off the terrace into a lowland pool,
// and a dirt path that climbs from the lowland to the shrine on two ramps of 20 cm per metre.
//
// Built the same way as terrain/generate.js: named broad forms first, noise only perturbs them.
//   * a STEP FIELD, constant per tile (sampled at the tile centre): the region's level. Two tiles in different regions disagree on their
//     shared edge, which is what makes risers and cliffs;
//   * a VERTEX FIELD per region, in whole levels: gentle rolling ground plus small named forms (a ridge, diamond mounds that meet in
//     saddles, a swale), limited so neighbouring vertices differ by at most one level. Tiles of one region share these corners, so inside
//     a region every edge joins and every tile is one of the five shapes; ramps take their exact 0.2 m-per-metre heights instead;
//   * the clamp to one level of spread per tile is kept as a safety net.
// Pure data, no three.js.
import { TileGrid, STEP, GROUND, NO_WATER } from './tile_grid.js';
import { fbm, clamp } from '../math/noise.js';

export const SCENE = {
  width: 48, depth: 40,
  // regions, metres (checked in order, first hit wins)
  levels: { pool: 0.0, low: 0.6, terrace: 2.6, west: 3.8, riseA: 3.4, riseB: 4.2, ridge: 3.8, plateau: 5.0 },
  // the shrine (15 x 10 m, authored facing north) faces WEST here, so it covers 10 x 15 tiles
  shrine: { tx: 34, tz: 2, w: 15, d: 10, facing: 'W' },
  ramps: [
    // lowland -> terrace: rises north from z = 37 (0.6 m) to z = 27 (2.6 m)
    { name: 'lower ramp', x0: 24, x1: 27, z0: 27, z1: 37, axis: 'z', from: 0.6, to: 2.6, highAtStart: true },
    // terrace -> plateau: rises east from x = 16 (2.6 m) to x = 28 (5.0 m)
    { name: 'upper ramp', x0: 16, x1: 28, z0: 8, z1: 11, axis: 'x', from: 2.6, to: 5.0, highAtStart: false },
  ],
  // the dirt path, as a polyline in metres (x, z); the ramps are on it
  path: [[25.5, 40], [25.5, 27], [24.5, 22], [20, 16.5], [16.2, 12.5], [15.5, 9.5], [28, 9.5], [34.2, 9.5]],
  pathHalfWidth: 1.15,
  // small landforms from the 1x1 tile-system sheet, in the low ground and on the terrace (metres; added to the rolling ground)
  //   ridges: a crest along a segment falling 0.2 m per metre on both sides (ramp tiles on the flanks)
  //   mounds: diamond (L1) hillocks centred on a tile corner; two of them touching diagonally make a saddle tile between them
  //   swales: a shallow dry valley along a segment
  forms: {
    ridges: [{ a: [30.5, 38.5], b: [38.5, 33.5], h: 0.6 }],
    mounds: [{ x: 43, z: 34, h: 0.4 }, { x: 42, z: 35, h: 0.4 }, { x: 31, z: 23, h: 0.4 }, { x: 30, z: 24, h: 0.4 }, { x: 17, z: 31, h: 0.4 }, { x: 18, z: 32, h: 0.4 }],
    swales: [{ a: [26.5, 18.5], b: [34.5, 24.5], h: 0.4 }],
  },
  stream: { x: 10, amp: 1.6, period: 11, halfWidth: 1.5, bedDrop: 0.4, waterDepth: 0.22, fallZ: 27 },
};

const SEED = 11;

/** Distance from (x, z) to a polyline. */
export function distToPolyline(pts, x, z) {
  let best = Infinity;
  for (let i = 0; i + 1 < pts.length; i++) {
    const [ax, az] = pts[i], [bx, bz] = pts[i + 1], dx = bx - ax, dz = bz - az;
    const t = clamp(((x - ax) * dx + (z - az) * dz) / (dx * dx + dz * dz || 1), 0, 1);
    best = Math.min(best, Math.hypot(x - ax - dx * t, z - az - dz * t));
  }
  return best;
}

const streamCentre = (z) => SCENE.stream.x + SCENE.stream.amp * Math.sin(z / SCENE.stream.period * Math.PI * 2 * 0.5 + 0.6);

/** Which region a point belongs to (used at tile centres). */
export function regionAt(x, z) {
  const n = (fbm(x * 0.16, z * 0.16, SEED, 3) - 0.5) * 6;              // about +-1.5 m wobble on region borders
  const s = SCENE.stream;
  if (z > 29.5 + n * 0.6 && x < 13 + n) return 'pool';
  if (z >= s.fallZ + n * 0.4 * (x > 18 ? 1 : 0)) {
    if (x > 35.5 + n * 0.5 && z < 30 + n * 0.5) return 'riseA';
    return 'low';
  }
  if (x < 7.2 + n * 0.8 && z < s.fallZ) return 'west';
  if (x >= 28 + n * 0.3 * (z < 7 ? 1 : 0) && z < 19 + n * 0.5) return 'plateau';
  if (x >= 20 + n * 0.5 && x < 28 && z < 7.6) return 'plateau';
  if (x >= 15.5 + n * 0.5 && x < 20 + n * 0.5 && z < 6.5 + n * 0.4) return 'ridge';
  if (x > 41 + n * 0.5 && z < 25.5 + n * 0.5) return 'riseB';
  if (x > 35.5 + n * 0.5 && z < 30 + n * 0.5) return 'riseA';
  return 'terrace';
}

function inRect(r, x, z) { return x >= r.x0 && x < r.x1 && z >= r.z0 && z < r.z1; }

/** Height (metres) a ramp gives at (x, z): linear, 1 level per metre along its axis. */
function rampHeight(r, x, z) {
  const t = r.axis === 'z' ? (r.highAtStart ? (r.z1 - z) / (r.z1 - r.z0) : (z - r.z0) / (r.z1 - r.z0))
    : (r.highAtStart ? (r.x1 - x) / (r.x1 - r.x0) : (x - r.x0) / (r.x1 - r.x0));
  return r.from + (r.to - r.from) * clamp(t, 0, 1);
}

/** Gentle rolling ground, metres (continuous; zero on pads, ramps and water). */
function rolling(x, z) { return (fbm(x * 0.06, z * 0.06, SEED + 5, 3) - 0.5) * 2.2; }

/** The named small landforms (SCENE.forms) at (x, z), metres. Slopes are 0.2 m per metre, so they quantize to ramp, corner and saddle tiles. */
function forms(x, z) {
  const f = SCENE.forms;
  let h = 0;
  for (const r of f.ridges) h = Math.max(h, r.h - 0.2 * distToPolyline([r.a, r.b], x, z));
  for (const m of f.mounds) h = Math.max(h, m.h - 0.2 * (Math.abs(x - m.x) + Math.abs(z - m.z)));
  for (const v of f.swales) h -= Math.max(0, v.h - 0.2 * distToPolyline([v.a, v.b], x, z));
  return h;
}

/**
 * A vertex field of levels (width + 1) x (depth + 1) (fn returns whole levels), limited so that every vertex differs from its eight neighbours by at
 * most one level. Inside one region all tiles share these corners, so the ground there never breaks into risers and every tile is one
 * of the five shapes (the 1x1 sheet's rule: at most one step across any edge). Risers and cliffs only come from region borders.
 */
function limitedField(width, depth, fn) {
  const W1 = width + 1, D1 = depth + 1, f = new Int16Array(W1 * D1);
  for (let z = 0; z < D1; z++) for (let x = 0; x < W1; x++) f[z * W1 + x] = fn(x, z);
  for (let pass = 0, changed = true; changed && pass < 64; pass++) {
    changed = false;
    for (let z = 0; z < D1; z++) for (let x = 0; x < W1; x++) {
      let lo = -Infinity, hi = Infinity;
      for (let dz = -1; dz <= 1; dz++) for (let dx = -1; dx <= 1; dx++) {
        const nx = x + dx, nz = z + dz;
        if ((dx || dz) && nx >= 0 && nz >= 0 && nx < W1 && nz < D1) { const v = f[nz * W1 + nx]; lo = Math.max(lo, v - 1); hi = Math.min(hi, v + 1); }
      }
      const i = z * W1 + x, v = Math.min(Math.max(f[i], lo), hi);
      if (v !== f[i]) { f[i] = v; changed = true; }
    }
  }
  return f;
}

export function buildExampleScene() {
  const { width, depth } = SCENE;
  const st0 = SCENE.stream, pad0 = SCENE.shrine;
  const padR = { x0: pad0.tx, z0: pad0.tz, x1: pad0.tx + pad0.d, z1: pad0.tz + pad0.w };
  const fields = {};
  const fieldFor = (reg) => (fields[reg] ??= limitedField(width, depth, (x, z) => {
    const L = (m) => Math.round(m / STEP);
    if (reg === 'stream') return L(Math.min(0, rolling(x, z)) * 0.3);
    if (reg === 'pool') return 0;
    if (reg === 'low' || reg === 'terrace') return L(rolling(x, z)) + L(forms(x, z));   // forms quantized on their own keep their exact shapes
    if (reg === 'plateau') {                                             // the plateau eases down to pad level near the pad
      const dPad = Math.max(padR.x0 - x, x - padR.x1, padR.z0 - z, z - padR.z1, 0);
      return L(rolling(x, z) * 0.45 * clamp(dPad / 4, 0, 1));
    }
    return L(rolling(x, z) * 0.6);
  }));
  const baseLevel = (reg) => Math.round((reg === 'stream' ? SCENE.levels.terrace - st0.bedDrop : SCENE.levels[reg]) / STEP);
  const grid = new TileGrid({ width, depth });
  const region = new Array(width * depth);
  const sh = SCENE.shrine, padRect = { x0: sh.tx, z0: sh.tz, x1: sh.tx + sh.d, z1: sh.tz + sh.w };   // turned W: d along x, w along z
  const st = SCENE.stream;

  for (let tz = 0; tz < depth; tz++) for (let tx = 0; tx < width; tx++) {
    const cx = tx + 0.5, cz = tz + 0.5, i = grid.index(tx, tz);
    const ramp = SCENE.ramps.find((r) => inRect(r, cx, cz));
    let reg = regionAt(cx, cz);
    const pad = inRect(padRect, cx, cz);
    const inStream = cz < st.fallZ && Math.abs(cx - streamCentre(cz)) < st.halfWidth && reg !== 'west' && !ramp;
    if (inStream) reg = 'stream';
    else if (reg === 'low' && Math.abs(cx - streamCentre(st.fallZ) + (cz - st.fallZ) * 0.35) < st.halfWidth + (cz - st.fallZ) * 0.25) reg = 'pool';   // the run-out below the fall
    region[i] = ramp ? 'ramp' : pad ? 'pad' : reg;

    const corners = [0, 0, 0, 0];
    for (let k = 0; k < 4; k++) {
      const [x, z] = grid.cornerXZ(tx, tz, k);
      if (ramp) corners[k] = Math.round(rampHeight(ramp, x, z) / STEP);
      else if (pad) corners[k] = Math.round(SCENE.levels.plateau / STEP);
      else corners[k] = baseLevel(reg) + fieldFor(reg)[z * (width + 1) + x];
    }
    const lo = Math.min(...corners);
    for (let k = 0; k < 4; k++) corners[k] = Math.min(corners[k], lo + 1);   // one level of spread at most: the five shapes
    grid.setCorners(tx, tz, corners);

    // ground and water
    const dPath = distToPolyline(SCENE.path, cx, cz);
    let g = GROUND.grass;
    if (region[i] === 'pad') g = GROUND.pad;
    else if (reg === 'stream' || reg === 'pool') g = GROUND.bed;
    else if (ramp || dPath < SCENE.pathHalfWidth) g = GROUND.path;
    grid.ground[i] = g;
    if (reg === 'stream') grid.water[i] = Math.round((SCENE.levels.terrace - st.bedDrop + st.waterDepth) / STEP);
    if (reg === 'pool') grid.water[i] = Math.round((SCENE.levels.pool + 0.4) / STEP);
  }

  // sand on the dry tiles touching water
  for (let tz = 0; tz < depth; tz++) for (let tx = 0; tx < width; tx++) {
    const i = grid.index(tx, tz);
    if (grid.water[i] !== NO_WATER || grid.ground[i] !== GROUND.grass) continue;
    for (const [dx, dz] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
      const x = tx + dx, z = tz + dz;
      if (grid.inBounds(x, z) && grid.water[grid.index(x, z)] !== NO_WATER && Math.abs(grid.base(x, z) - grid.base(tx, tz)) <= 3) { grid.ground[i] = GROUND.sand; break; }
    }
  }

  return {
    grid, region, padRect, shrine: sh, path: SCENE.path, ramps: SCENE.ramps, streamCentre,
    features: [
      { name: 'shrine pad', x: (padRect.x0 + padRect.x1) / 2, z: (padRect.z0 + padRect.z1) / 2 },
      { name: 'waterfall', x: streamCentre(st.fallZ), z: st.fallZ },
      { name: 'lower ramp', x: 25.5, z: 32 },
      { name: 'upper ramp', x: 22, z: 9.5 },
      { name: 'rocky rise', x: 41, z: 24 },
      { name: 'pool', x: 6, z: 35 },
      { name: 'ridge and saddles', x: 37, z: 35 },
    ],
  };
}
