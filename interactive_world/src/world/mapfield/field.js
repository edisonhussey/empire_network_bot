// MapField: the land as a pure, lazily evaluated description. Given a spec (rivers, roads, forests, rocks, outcrops, fields, buildings, props)
// it answers, for any x,y in nodes: how high is the ground, is it water or a bridge, what surface is it, how close is the nearest solid, how
// costly is it to walk. Nothing is stored per node, so it scales from a 50-node demo to a 1286-node map; trees are scattered per 8-node chunk
// on demand. The pathfinder, the ground mesh, the water and the props are all derived from these questions, so they always agree.
//
// Units: x,y in grid nodes (y becomes scene z, 1 node = CELL = 10 m); heights in metres; `m(metres)` turns a physical size into nodes. Rotations are degrees in the spec and radians on placed props
// (rot = 0 faces +y, i.e. scene +z). Bridges are created automatically where a road crosses a river.
import { Path } from './curves.js';
import { SolidIndex } from './solids.js';
import { SpatialIndex } from '../spatial.js';
import { fbm, valueNoise, hash2, smoothstep } from './noise.js';
import { WATER_LEVEL, BRIDGE, BUILDING_SOLIDS, STONE_ASSETS } from './constants.js';
import { CELL } from '../config.js';

const DEG = Math.PI / 180;
const CHUNK = 8;
const TREE_CLEARANCE = 0.5;   // nodes a squad keeps from a single tree
const WATER_MARGIN = 0.5;     // nodes squads keep from water: rivers are narrower than a node, so without it the lattice could step across one between two dry nodes
const BRIDGE_SLACK = 0.5;     // the walkable strip of a bridge extends this far past its ends, to cover that margin
const BUCKET = 8;
const m = (metres) => metres / CELL;

export class MapField {
  constructor(spec = {}) {
    this.spec = spec;
    this.size = spec.size ?? 50;
    this.pad = spec.pad ?? 3;
    this.seed = spec.seed ?? 1;
    this.hills = { amp: 1.5, scale: 11, ...spec.hills };   // metres of relief; wavelength in nodes
    this.solids = new SolidIndex();
    this.eager = new SpatialIndex(BUCKET);
    this.clearings = new Map();
    this.flats = new Map();
    this.bridgeCells = new Map();
    this.chunks = new Map();
    this.nextTreeId = 1;
    this.rivers = (spec.rivers ?? []).map((r) => ({ width: r.width ?? 0.75, depth: r.depth ?? 1.5, path: new Path(r.points, { reach: (r.width ?? 0.75) / 2 + 4.5 }) }));
    this.roads = (spec.roads ?? []).map((r) => ({ width: r.width ?? 0.3, path: new Path(r.points, { reach: (r.width ?? 0.3) / 2 + 1.5 }) }));
    this.bridges = [];
    this.forests = (spec.forests ?? []).map((f) => ({ density: 0.7, mix: 0.5, ...f }));
    this.#buildBridges();
    this.#buildBanks();
    for (const f of spec.fields ?? []) this.#addField(f);
    for (const r of spec.rocks ?? []) this.#addRock(r);
    for (const o of spec.outcrops ?? []) this.#addOutcrop(o);
    for (const b of spec.buildings ?? []) this.#addBuilding(b);
    for (const p of spec.props ?? []) this.#prop({ rot: 0, ...p, rot: (p.rot ?? 0) * DEG });
    for (const c of spec.clearings ?? []) this.addClearing(c.x, c.y, c.r);
    for (const f of spec.flats ?? []) this.addFlat(f.x, f.y, f.r);
  }

  // ---- registering things ----------------------------------------------------------------------------------------------------------------------
  #prop(p) {
    const q = { sx: 1, sy: 1, sz: 1, tint: 1, ...p };
    this.eager.insert(q);
    return q;
  }

  #addRock({ x, y, size = 1, variant = 0 }) {
    const names = STONE_ASSETS[Math.min(3, Math.max(1, size))];
    this.#prop({ asset: names[variant % names.length], x, y, rot: hash2(x * 7, y * 13, this.seed) * Math.PI * 2 });
    this.solids.add({ shape: 'circle', kind: 'rock', x, y, r: 0.42 + 0.34 * size });
  }

  #addOutcrop({ kind = 'bump', x, y, rot = 0, h = 1 }) {
    const a = rot * DEG, sy = h;   // h scales the piece's height (1 = as modelled)
    if (kind === 'bump') { this.#prop({ asset: 'rocky_bump', x, y, rot: a, sy }); this.solids.add({ shape: 'circle', kind: 'rock', x, y, r: 1.0 }); }
    else if (kind === 'cliff' || kind === 'cliff_tall') { this.#prop({ asset: kind === 'cliff' ? 'cliff_edge' : 'cliff_tall', x, y, rot: a, sy }); this.solids.add({ shape: 'box', kind: 'rock', x, y, hx: 1.4, hy: 0.95, rot: -a }); }
    else if (kind === 'slope') this.#prop({ asset: 'slope_piece', x, y, rot: a, sy });
  }

  #addBuilding({ kind, x, y, rot = 0, height = 1 }) {
    const a = rot * DEG, s = BUILDING_SOLIDS[kind];
    this.#prop({ asset: kind, x, y, rot: a, sy: height });   // height scales the building (1 = as modelled)
    if (s?.r) this.solids.add({ shape: 'circle', kind: 'building', x, y, r: s.r });
    else if (s) this.solids.add({ shape: 'box', kind: 'building', x, y, hx: s.hx, hy: s.hy, rot: -a });
    this.addFlat(x, y, 1.2);
  }

  #addField({ x, y, w = 2, h = 2, rot = 0, kind = 'crop', fence = true }) {
    const a = rot * DEG, c = Math.cos(a), s = Math.sin(a);
    const at = (lx, ly) => ({ x: x + lx * s + ly * c, y: y + lx * c - ly * s });   // local (x right, y forward) to node coordinates
    const p = at(0, 0);
    this.#prop({ asset: kind === 'hay' ? 'hay_bales' : 'crop_field', x: p.x, y: p.y, rot: a, sx: w / 2, sz: h / 2 });
    this.solids.add({ shape: 'box', kind: 'field', x, y, hx: w / 2 - 0.05, hy: h / 2 - 0.05, rot: -a });
    this.addFlat(x, y, Math.max(w, h) / 2 + 0.4);
    if (!fence) return;
    // fence along the four sides, one segment per node, with a gap in the middle of the front side
    const sides = [[0, h / 2, 0], [0, -h / 2, 0], [w / 2, 0, Math.PI / 2], [-w / 2, 0, Math.PI / 2]];
    for (const [lx, ly, extra] of sides) {
      const len = extra ? h : w, n = Math.max(1, Math.round(len));
      for (let i = 0; i < n; i++) {
        const t = (i + 0.5) / n - 0.5;
        if (ly > 0 && !extra && Math.abs(t) < 0.5 / n) continue;
        const q = extra ? at(lx, t * len) : at(t * len, ly);
        this.#prop({ asset: 'fence', x: q.x, y: q.y, rot: a + (extra ? Math.PI / 2 : 0) + Math.PI / 2, sx: len / n });
      }
    }
  }

  /** Area where trees are not scattered (castles, NPC sites). */
  addClearing(x, y, r) {
    const k = `${Math.floor(x / BUCKET)},${Math.floor(y / BUCKET)}`;
    (this.clearings.get(k) ?? this.clearings.set(k, []).get(k)).push({ x, y, r });
    this.chunks.clear();
  }

  /** A patch the ground is levelled to (height 0) with a soft edge: castles and buildings sit on one. */
  addFlat(x, y, r) {
    for (let by = Math.floor((y - r - 3) / BUCKET); by <= Math.floor((y + r + 3) / BUCKET); by++) {
      for (let bx = Math.floor((x - r - 3) / BUCKET); bx <= Math.floor((x + r + 3) / BUCKET); bx++) (this.flats.get(`${bx},${by}`) ?? this.flats.set(`${bx},${by}`, []).get(`${bx},${by}`)).push({ x, y, r });
    }
  }

  /** A castle / NPC site: keep trees away and level the ground. */
  addEntity(e, half) {
    this.addClearing(e.x, e.y, half + 2.4);
    this.addFlat(e.x, e.y, half + 1.1);
  }

  // ---- bridges: wherever a road runs through a river ----------------------------------------------------------------------------------------------
  #buildBridges() {
    for (const road of this.roads) {
      const pts = road.path.pts;
      let run = null;
      const flush = (end) => {
        if (!run) return;
        // the crossing point is where the road is closest to the river's centreline; the bridge spans straight across the river there
        let k = run.i0, best = Infinity;
        for (let i = run.i0; i <= end; i++) { const r = this.riverAt(pts[i].x, pts[i].y); if (r && r.d < best) { best = r.d; k = i; } }
        const c = pts[k], info = this.riverAt(c.x, c.y);
        let dx = -info.dy, dy = info.dx;
        const a = pts[Math.max(0, k - 1)], b = pts[Math.min(pts.length - 1, k + 1)];
        if (dx * (b.x - a.x) + dy * (b.y - a.y) < 0) { dx = -dx; dy = -dy; }
        const len = info.river.width + m(4);   // the water plus 2 m of landing each side (the bank slopes down over 2 m)
        const tiles = BRIDGE.lengths.find((n) => n >= len) ?? BRIDGE.lengths[BRIDGE.lengths.length - 1];
        const ends = [-1, 1].map((e) => this.heightAt(c.x + dx * (len / 2 + m(1)) * e, c.y + dy * (len / 2 + m(1)) * e));
        const br = { x: c.x, y: c.y, dx, dy, len, tiles, scale: len / tiles, asset: `bridge_${tiles}`, rot: Math.atan2(dx, dy), half: BRIDGE.halfWidth, base: (ends[0] + ends[1]) / 2 };
        this.bridges.push(br);
        const reach = len / 2 + 2 + BRIDGE_SLACK;
        for (let by = Math.floor((br.y - reach) / BUCKET); by <= Math.floor((br.y + reach) / BUCKET); by++) {
          for (let bx = Math.floor((br.x - reach) / BUCKET); bx <= Math.floor((br.x + reach) / BUCKET); bx++) (this.bridgeCells.get(`${bx},${by}`) ?? this.bridgeCells.set(`${bx},${by}`, []).get(`${bx},${by}`)).push(br);
        }
        this.#prop({ asset: br.asset, x: br.x, y: br.y, rot: br.rot, sz: br.scale, bridge: true, h: br.base });
        run = null;
      };
      for (let i = 0; i < pts.length; i++) {
        const info = this.riverAt(pts[i].x, pts[i].y);
        const wet = info && info.edge < 0.5;
        if (wet && !run) run = { i0: Math.max(0, i - 2) };
        if (!wet && run) flush(Math.min(pts.length - 1, i + 1));
      }
      flush(pts.length - 1);
    }
  }

  /** Stones along the banks of every river (alternating sides, skipping bridges). */
  #buildBanks() {
    for (const r of this.rivers) {
      for (let s = 1.5; s < r.path.length; s += 1.4) {
        const p = r.path.pointAt(s), side = hash2(Math.floor(s), 3, this.seed) < 0.5 ? -1 : 1;
        if (hash2(Math.floor(s * 3), 9, this.seed) > 0.72) continue;
        const off = r.width / 2 + m(2.5), x = p.x - p.dy * off * side, y = p.y + p.dx * off * side;
        if (this.bridgeAt(x, y) || this.bridgeAt(p.x, p.y)) continue;
        this.#prop({ asset: 'bank_stones', x, y, rot: Math.atan2(p.dx, p.dy) + Math.PI / 2, sx: 1 + hash2(Math.floor(s), 5, this.seed) * 0.5 });
      }
    }
  }

  bridgeAt(x, y, slack = 0) {
    const list = this.bridgeCells.get(`${Math.floor(x / BUCKET)},${Math.floor(y / BUCKET)}`);
    if (!list) return null;
    for (const b of list) {
      const rx = x - b.x, ry = y - b.y, along = rx * b.dx + ry * b.dy, across = -rx * b.dy + ry * b.dx;
      if (Math.abs(along) <= b.len / 2 + slack && Math.abs(across) <= b.half) return { bridge: b, along };
    }
    return null;
  }

  // ---- questions about a point -------------------------------------------------------------------------------------------------------------------------
  /** Nearest river: { edge (distance to the water's edge; < 0 is in the water), river, s, dx, dy } or null. */
  riverAt(x, y) {
    let best = null;
    for (const r of this.rivers) {
      const n = r.path.nearest(x, y);
      if (!n) continue;
      const edge = n.d - r.width / 2;
      if (!best || edge < best.edge) best = { edge, river: r, s: n.s, dx: n.dx, dy: n.dy, d: n.d };
    }
    return best;
  }

  roadAt(x, y) {
    let best = null;
    for (const r of this.roads) {
      const n = r.path.nearest(x, y);
      if (n && (!best || n.d - r.width / 2 < best.edge)) best = { edge: n.d - r.width / 2, road: r, d: n.d };
    }
    return best;
  }

  isWater(x, y) { const r = this.riverAt(x, y); return !!r && r.edge < 0; }
  isBridge(x, y) { return !!this.bridgeAt(x, y, BRIDGE_SLACK); }   // for routing: includes the approach to the deck

  /** Ground height (scene units): gentle hills, levelled under flats, carved into river channels. */
  heightAt(x, y) {
    let h = this.hills.amp * (fbm(x / this.hills.scale, y / this.hills.scale, this.seed, 3) - 0.5) * 2;
    const flats = this.flats.get(`${Math.floor(x / BUCKET)},${Math.floor(y / BUCKET)}`);
    if (flats) {
      let w = 0;
      for (const f of flats) w = Math.max(w, 1 - smoothstep(f.r, f.r + 1.8, Math.hypot(x - f.x, y - f.y)));
      h *= 1 - w;
    }
    const river = this.riverAt(x, y);
    if (river) {   // the bed sits a fixed depth under the water level whatever the hills do; the banks blend from the land down to it
      const e = river.edge, w = e < 0 ? 1 : 1 - smoothstep(0, m(2), e);   // steep banks: down to the bed over 2 m
      h = h * (1 - w) + (WATER_LEVEL - river.river.depth) * w;
    }
    return h;
  }

  /** Height someone walking at (x, y) stands at: the bridge deck on a bridge, otherwise the ground. */
  walkHeightAt(x, y) {
    const b = this.bridgeAt(x, y);
    if (b) return b.bridge.base + BRIDGE.deck + BRIDGE.arch * Math.sin(Math.PI * (b.along / b.bridge.len + 0.5));
    return this.heightAt(x, y);
  }

  /** Surface weights in [0, 1] for painting the ground: road, bank (sand), soil (ploughed), rock. */
  surfaceAt(x, y) {
    const river = this.riverAt(x, y), road = this.roadAt(x, y);
    return {
      bank: river ? (river.edge < 0 ? 1 : 1 - smoothstep(m(1), m(6), river.edge)) : 0,
      road: road ? 1 - smoothstep(-m(0.4), m(1.5), road.edge) : 0,
      soil: 1 - smoothstep(0, 0.45, this.solids.nearest(x, y, 'field')),
      rock: 1 - smoothstep(0, 0.9, this.solids.nearest(x, y, 'rock')),
    };
  }

  /** Walking cost multiplier: roads are quicker, the bank a little slower. */
  costAt(x, y) {
    const r = this.roadAt(x, y);
    if (r && r.edge < 0.6) return 0.6;   // the lattice only samples the road at whole nodes, so its effective width is a little more than its drawn width
    return 1;
  }

  // ---- trees: scattered lazily, 8x8-node chunks -----------------------------------------------------------------------------------------------------------
  #forestWeight(x, y) {
    let w = 0;
    for (const f of this.forests) {
      let v = 0;
      if (f.shape === 'circle') v = 1 - smoothstep(f.r * 0.45, f.r, Math.hypot(x - f.x, y - f.y));
      else if (f.shape === 'poly') v = pointInPoly(x, y, f.points) ? 1 : 0;
      else if (f.shape === 'noise') v = smoothstep(0.64 - (f.cover ?? 0.35) * 0.5, 0.74 - (f.cover ?? 0.35) * 0.5, fbm(x / (f.scale ?? 14), y / (f.scale ?? 14), this.seed + 77, 4));
      w = Math.max(w, v * f.density * (1 - 0.25 * valueNoise(x * 0.4, y * 0.4, this.seed + 5)));
    }
    return w;
  }

  #treeChunk(cx, cy) {
    const key = cx * 100003 + cy;
    let c = this.chunks.get(key);
    if (c) return c;
    c = { trees: [], cover: [] };
    if (this.forests.length || this.spec.cover !== false) {
      const lim = this.pad + this.size;
      for (let j = 0; j < CHUNK; j++) for (let i = 0; i < CHUNK; i++) {
        const gx = cx * CHUNK + i, gy = cy * CHUNK + j;
        this.#cover(c.cover, gx, gy);
        if (!this.forests.length) continue;
        const x = gx + 0.5 + (hash2(gx, gy, this.seed + 1) - 0.5) * 0.9, y = gy + 0.5 + (hash2(gx, gy, this.seed + 2) - 0.5) * 0.9;
        if (x < -this.pad || y < -this.pad || x > lim || y > lim) continue;
        const w = this.#forestWeight(x, y);
        if (w <= 0.02 || hash2(gx, gy, this.seed + 3) > w * 0.62) continue;   // ~0.6 trees per node at full density
        const river = this.riverAt(x, y);
        if (river && river.edge < 1.0) continue;
        const road = this.roadAt(x, y);
        if (road && road.edge < 0.95) continue;
        if (this.solids.nearest(x, y) < 0.95 || this.bridgeAt(x, y) || this.#inClearing(x, y)) continue;
        // the only tree is the pine (assets/pine.glb), one height; each gets a little natural variation in width and height
        c.trees.push({ id: this.nextTreeId++, asset: 'pine', x, y, rot: hash2(gx, gy, this.seed + 6) * Math.PI * 2, tint: 0.9 + hash2(gx, gy, this.seed + 7) * 0.2, sx: 0.92 + hash2(gx, gy, this.seed + 8) * 0.16, sy: 0.9 + hash2(gx, gy, this.seed + 10) * 0.2, sz: 1, r: 0.32 });
      }
    }
    this.chunks.set(key, c);
    if (this.chunks.size > 9000) this.chunks.delete(this.chunks.keys().next().value);
    return c;
  }

  /** Ground cover: grass tufts (some with flowers) on free grass, in the bare spaces and everywhere under trees too. */
  #cover(out, gx, gy) {
    const h1 = hash2(gx, gy, this.seed + 21);
    if (h1 > 0.5) return;
    const x = gx + hash2(gx, gy, this.seed + 22), y = gy + hash2(gx, gy, this.seed + 23);
    if (x < -this.pad || y < -this.pad || x > this.size + this.pad || y > this.size + this.pad) return;
    const river = this.riverAt(x, y);
    if (river && river.edge < 1.2) return;
    const road = this.roadAt(x, y);
    if (road && road.edge < 0.55) return;
    if (this.solids.nearest(x, y) < 0.3 || this.bridgeAt(x, y)) return;
    out.push({ asset: h1 < 0.04 ? 'tuft_b' : 'tuft_a', x, y, rot: hash2(gx, gy, this.seed + 24) * 6.28, tint: 0.85 + hash2(gx, gy, this.seed + 25) * 0.3, sx: 0.8 + hash2(gx, gy, this.seed + 26) * 0.7, sy: 0.8 + hash2(gx, gy, this.seed + 27) * 0.8, sz: 1 });
  }

  #inClearing(x, y) {
    for (let by = Math.floor(y / BUCKET) - 1; by <= Math.floor(y / BUCKET) + 1; by++) {
      for (let bx = Math.floor(x / BUCKET) - 1; bx <= Math.floor(x / BUCKET) + 1; bx++) {
        for (const c of this.clearings.get(`${bx},${by}`) ?? []) if (Math.hypot(x - c.x, y - c.y) < c.r) return true;
      }
    }
    return false;
  }

  /** Distance to the nearest tree (edge to point; Infinity when none is near). */
  treeDistance(x, y) {
    let best = Infinity;
    const cx = Math.floor(x / CHUNK), cy = Math.floor(y / CHUNK);
    for (let j = -1; j <= 1; j++) for (let i = -1; i <= 1; i++) {
      for (const t of this.#treeChunk(cx + i, cy + j).trees) { const d = Math.hypot(x - t.x, y - t.y) - t.r; if (d < best) best = d; }
    }
    return best;
  }

  /** Distance to the nearest solid thing, trees included (Infinity when nothing is near). */
  solidDistance(x, y) { return Math.min(this.solids.nearest(x, y), this.treeDistance(x, y)); }

  /**
   * Can a squad stand at (x, y)? It keeps `clearance` nodes from rocks, buildings and fields, and a smaller margin from single trees (so
   * it can pass along a road through a forest). Water blocks except under a bridge.
   */
  blockedAt(x, y, clearance = 0.9, treeClearance = TREE_CLEARANCE) {
    if (this.isBridge(x, y)) return false;
    const river = this.riverAt(x, y);
    if (river && river.edge < WATER_MARGIN) return true;
    if (clearance > 0 && this.solids.nearest(x, y) < clearance) return true;
    return treeClearance > 0 && this.treeDistance(x, y) < treeClearance;
  }

  // ---- what to draw -------------------------------------------------------------------------------------------------------------------------------------
  /** Every placed piece inside a rect of nodes (hand-placed pieces and the trees of the covered chunks): { asset, x, y, rot, sx, sy, sz, tint }. */
  propsIn(rect) {
    const out = this.eager.query({ x0: rect.x0 - 2, y0: rect.y0 - 2, x1: rect.x1 + 2, y1: rect.y1 + 2 });
    for (let cy = Math.floor(rect.y0 / CHUNK); cy <= Math.floor(rect.y1 / CHUNK); cy++) {
      for (let cx = Math.floor(rect.x0 / CHUNK); cx <= Math.floor(rect.x1 / CHUNK); cx++) {
        const chunk = this.#treeChunk(cx, cy);
        for (const t of chunk.trees) if (t.x >= rect.x0 && t.x <= rect.x1 && t.y >= rect.y0 && t.y <= rect.y1) out.push(t);
        for (const t of chunk.cover) if (t.x >= rect.x0 && t.x <= rect.x1 && t.y >= rect.y0 && t.y <= rect.y1) out.push(t);
      }
    }
    return out;
  }

  /** The bitmap of the boundaries for a rect of nodes at `res` samples per node: bit 1 = water, bit 2 = blocked by a solid; plus heights. */
  rasterise(rect, { res = 1, clearance = 0.9 } = {}) {
    const w = Math.ceil((rect.x1 - rect.x0) * res) + 1, h = Math.ceil((rect.y1 - rect.y0) * res) + 1;
    const mask = new Uint8Array(w * h), height = new Float32Array(w * h);
    for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) {
      const x = rect.x0 + i / res, y = rect.y0 + j / res;
      const bridge = this.isBridge(x, y), water = !bridge && this.isWater(x, y);
      mask[j * w + i] = (water ? 1 : 0) | (!bridge && !water && this.blockedAt(x, y, clearance) ? 2 : 0);
      height[j * w + i] = this.walkHeightAt(x, y);
    }
    return { x0: rect.x0, y0: rect.y0, res, w, h, mask, height };
  }
}

function pointInPoly(x, y, pts) {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const [xi, yi] = pts[i], [xj, yj] = pts[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}
