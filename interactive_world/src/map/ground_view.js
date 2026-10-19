// The ground: a heightfield drawn in 12-node chunks, built lazily from the land (MapField) for the render window only. Nearby chunks are
// dense, distant ones coarse, and each chunk carries a skirt so neighbouring detail levels never show cracks. Vertex colours paint grass,
// roads, river banks, ploughed soil and rocky ground from the field's surface weights, so the ground always matches the routes and props.
import * as THREE from 'three';
import { CELL, LEGACY_SCALE as S } from '../world/config.js';
import { WATER_LEVEL } from '../world/mapfield/constants.js';
import { fbm } from '../world/mapfield/noise.js';

const CHUNK = 12, BUDGET = 2, MAX_CHUNKS = 520;
const col = (h) => new THREE.Color(h);
const GRASS_A = col('#7db03a'), GRASS_B = col('#69992f'), GRASS_C = col('#8bb845'), SAND = col('#d8c58c'), MUD = col('#8d7a52'), DIRT = col('#b48d57'), SOIL = col('#7d5a38'), ROCKY = col('#82905c');
const lerp = (a, b, t) => a.clone().lerp(b, t);

export function createGroundView({ scene, world }) {
  const group = new THREE.Group();
  scene.add(group);
  const material = new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 1, metalness: 0, side: THREE.DoubleSide });
  const chunks = new Map();   // key -> { mesh, res, version }
  const queue = [];
  let want = [];

  function paint(field, x, y, h, out) {
    const s = field.surfaceAt(x, y);
    const n = fbm(x * 0.32, y * 0.32, 5, 2), n2 = fbm(x * 0.9 + 7, y * 0.9, 6, 2);
    const c = lerp(lerp(GRASS_A, GRASS_B, n), GRASS_C, Math.max(0, n2 - 0.55) * 1.4);
    c.lerp(ROCKY, s.rock * 0.55);
    c.lerp(h < WATER_LEVEL ? MUD : SAND, Math.min(1, s.bank * 0.95));
    c.lerp(SOIL, s.soil);
    c.lerp(DIRT, s.road * 0.95);
    c.multiplyScalar(0.96 + (h / S) * 0.18);   // a touch lighter on the heights
    out.push(c.r, c.g, c.b);
  }

  function build(cx, cy, res) {
    const field = world.field;
    const n = Math.max(2, Math.ceil(CHUNK * res)), step = CHUNK / n;
    const x0 = cx * CHUNK, y0 = cy * CHUNK;
    const stride = n + 1, hs = n + 3;   // heights are sampled one ring beyond the chunk so border normals match the neighbours' (no seams)
    const H = new Float32Array(hs * hs);
    for (let j = -1; j <= n + 1; j++) for (let i = -1; i <= n + 1; i++) H[(j + 1) * hs + i + 1] = field ? field.heightAt(x0 + i * step, y0 + j * step) : 0;
    const pos = [], nor = [], colors = [], idx = [];
    const inv = 1 / (2 * step * CELL);
    for (let j = 0; j <= n; j++) for (let i = 0; i <= n; i++) {
      const h = H[(j + 1) * hs + i + 1], x = x0 + i * step, y = y0 + j * step;
      const gx = (H[(j + 1) * hs + i] - H[(j + 1) * hs + i + 2]) * inv, gz = (H[j * hs + i + 1] - H[(j + 2) * hs + i + 1]) * inv;
      const l = Math.hypot(gx, 1, gz);
      pos.push(x * CELL, h, y * CELL);
      nor.push(gx / l, 1 / l, gz / l);
      if (field) paint(field, x, y, h, colors); else colors.push(0.45, 0.65, 0.2);
    }
    for (let j = 0; j < n; j++) for (let i = 0; i < n; i++) {
      const a = j * stride + i, b = a + 1, c = a + stride, d = c + 1;
      idx.push(a, c, b, b, c, d);
    }
    const skirt = (vs) => {   // drop a skirt under a border so LOD seams are hidden
      const base = pos.length / 3;
      for (const v of vs) { pos.push(pos[v * 3], pos[v * 3 + 1] - 0.3 * S, pos[v * 3 + 2]); nor.push(nor[v * 3], nor[v * 3 + 1], nor[v * 3 + 2]); colors.push(colors[v * 3] * 0.8, colors[v * 3 + 1] * 0.8, colors[v * 3 + 2] * 0.8); }
      for (let k = 0; k < vs.length - 1; k++) idx.push(vs[k], base + k, vs[k + 1], vs[k + 1], base + k, base + k + 1);
    };
    const south = [], north = [], west = [], east = [];
    for (let i = 0; i <= n; i++) { north.push(i); south.push(n * stride + i); west.push(i * stride); east.push(i * stride + n); }
    skirt(north.slice().reverse()); skirt(south); skirt(west); skirt(east.slice().reverse());
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
    g.setAttribute('normal', new THREE.Float32BufferAttribute(nor, 3));
    g.setAttribute('color', new THREE.Float32BufferAttribute(colors, 3));
    g.setIndex(idx);
    const mesh = new THREE.Mesh(g, material);
    mesh.receiveShadow = true;
    mesh.frustumCulled = true;
    return mesh;
  }

  /** Decide which chunks the window needs, at what detail (by distance from `focus`, in nodes). Building happens in update(). */
  function sync(rect, focus) {
    want = [];
    for (let cy = Math.floor(rect.y0 / CHUNK); cy <= Math.floor(rect.y1 / CHUNK); cy++) {
      for (let cx = Math.floor(rect.x0 / CHUNK); cx <= Math.floor(rect.x1 / CHUNK); cx++) {
        const d = Math.hypot(cx * CHUNK + CHUNK / 2 - focus.x, cy * CHUNK + CHUNK / 2 - focus.y);
        want.push({ cx, cy, d, res: d < 28 ? 3.6 : d < 60 ? 1.8 : 0.9 });   // vertices per node: rivers are 5-10 m wide, so the near ground needs ~3 m spacing
      }
    }
    want.sort((a, b) => a.d - b.d);
    const need = new Set();
    queue.length = 0;
    for (const w of want) {
      const key = `${w.cx},${w.cy}`;
      need.add(key);
      const c = chunks.get(key);
      if (c) c.mesh.visible = true;
      if (!c || c.res !== w.res || c.version !== world.version) queue.push(w);
    }
    for (const [key, c] of chunks) if (!need.has(key)) c.mesh.visible = false;
    if (chunks.size > MAX_CHUNKS) {
      for (const [key, c] of chunks) { if (chunks.size <= MAX_CHUNKS * 0.8) break; if (!c.mesh.visible) { group.remove(c.mesh); c.mesh.geometry.dispose(); chunks.delete(key); } }
    }
  }

  /** Build a couple of queued chunks per frame (nearest first), so panning never stalls. */
  function update() {
    for (let k = 0; k < BUDGET && queue.length; k++) {
      const w = queue.shift(), key = `${w.cx},${w.cy}`, old = chunks.get(key);
      if (old) { group.remove(old.mesh); old.mesh.geometry.dispose(); }
      const mesh = build(w.cx, w.cy, w.res);
      group.add(mesh);
      chunks.set(key, { mesh, res: w.res, version: world.version });
    }
  }
  return { sync, update, stats: () => ({ chunks: chunks.size, queued: queue.length }), pending: () => queue.length };
}
