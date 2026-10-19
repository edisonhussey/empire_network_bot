// Terrain features, windowed. Boulders and walls are drawn as faceted rocks, instanced (a handful of draw calls however many there are);
// other kinds go through the drawers registry. River and bridge drawers are flat PLACEHOLDERS until their real scene is designed.
import * as THREE from 'three';
import { CELL, LEGACY_SCALE as S } from '../world/config.js';
import { SpatialIndex } from '../world/spatial.js';
import { intersects } from '../world/rect.js';
import { mulberry } from './rand.js';
import { hullGeometry } from '../render/outline.js';

const VARIANTS = 5, CAP = 2500;
const TONES = ['#8d8d90', '#7d7e82', '#9a9a98', '#74757a'].map((c) => new THREE.Color(c));

function rockGeometry(rng) {
  const g = new THREE.IcosahedronGeometry(1, 1).toNonIndexed();
  const p = g.attributes.position;
  const jitter = new Map(); // same jitter for coincident vertices, so the rock stays closed
  for (let i = 0; i < p.count; i++) {
    const key = `${p.getX(i).toFixed(3)},${p.getY(i).toFixed(3)},${p.getZ(i).toFixed(3)}`;
    if (!jitter.has(key)) jitter.set(key, 0.82 + rng() * 0.36);
    const k = jitter.get(key);
    p.setXYZ(i, p.getX(i) * k, p.getY(i) * k, p.getZ(i) * k);
  }
  g.deleteAttribute('uv');
  g.computeVertexNormals();
  return g;
}

/** Drawers for non-instanced kinds: kind -> (feature) => Object3D. Replace the placeholders when the scene art arrives. */
export const drawers = {
  river: (f) => {
    const m = new THREE.Mesh(new THREE.BoxGeometry(f.w * CELL, 0.12 * S, f.h * CELL), new THREE.MeshStandardMaterial({ color: '#4aa3d8', roughness: 0.35, transparent: true, opacity: 0.88 }));
    m.position.set((f.x + (f.w - 1) / 2) * CELL, -0.02 * S, (f.y + (f.h - 1) / 2) * CELL);
    m.receiveShadow = true;
    return m;
  },
  bridge: (f) => {
    const m = new THREE.Mesh(new THREE.BoxGeometry((f.w + 0.4) * CELL, 0.2 * S, (f.h + 0.2) * CELL), new THREE.MeshStandardMaterial({ color: '#8a6a44', roughness: 0.9 }));
    m.position.set((f.x + (f.w - 1) / 2) * CELL, 0.05 * S, (f.y + (f.h - 1) / 2) * CELL);
    m.castShadow = m.receiveShadow = true;
    return m;
  },
};

export function createTerrainView({ scene, world, outlineMat }) {
  const group = new THREE.Group();
  scene.add(group);

  // rock chunks: one rock per 2x2 nodes of every boulder / wall box, laid out once and indexed for window queries
  const rocks = new SpatialIndex(8);
  const buildRocks = () => {
    for (const f of world.terrain.features) {
      if (f.kind !== 'boulder' && f.kind !== 'wall') continue;
      const rng = mulberry((f.x * 73856093) ^ (f.y * 19349663) ^ (f.w * 83492791));
      for (let j = 0; j < f.h; j += 2) for (let i = 0; i < f.w; i += 2) {
        const cw = Math.min(2, f.w - i), ch = Math.min(2, f.h - j), height = (0.75 + rng() * 0.7) * CELL * 0.95;
        rocks.insert({ x: f.x + i + (cw - 1) / 2, y: f.y + j + (ch - 1) / 2, sx: cw * CELL * 0.6, sy: height, sz: ch * CELL * 0.6, rot: rng() * Math.PI, variant: Math.floor(rng() * VARIANTS), tone: Math.floor(rng() * TONES.length) });
      }
    }
  };
  buildRocks();

  const rng = mulberry(77);
  const mat = new THREE.MeshStandardMaterial({ color: '#ffffff', roughness: 0.95, flatShading: true });
  const meshes = [], hulls = [];
  for (let v = 0; v < VARIANTS; v++) {
    const geo = rockGeometry(rng);
    const m = new THREE.InstancedMesh(geo, mat, CAP);
    m.castShadow = m.receiveShadow = true; m.frustumCulled = false; m.count = 0;
    m.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(CAP * 3), 3);
    const h = new THREE.InstancedMesh(hullGeometry(geo), outlineMat, CAP);
    h.frustumCulled = false; h.count = 0;
    group.add(m, h);
    meshes.push(m); hulls.push(h);
  }

  // other kinds: built once, shown while their box touches the window
  const objects = [];
  for (const f of world.terrain.features) {
    const draw = drawers[f.kind];
    if (!draw) continue;
    const o = draw(f);
    group.add(o);
    objects.push({ f, o, box: { x0: f.x - 0.5, y0: f.y - 0.5, x1: f.x + f.w - 0.5, y1: f.y + f.h - 0.5 } });
  }

  const dummy = new THREE.Object3D();
  function sync(rect) {
    const counts = new Array(VARIANTS).fill(0);
    for (const r of rocks.query(rect)) {
      const v = r.variant, i = counts[v];
      if (i >= CAP) continue;
      dummy.position.set(r.x * CELL, r.sy * 0.32, r.y * CELL);
      dummy.rotation.set(0, r.rot, 0);
      dummy.scale.set(r.sx, r.sy, r.sz);
      dummy.updateMatrix();
      meshes[v].setMatrixAt(i, dummy.matrix);
      hulls[v].setMatrixAt(i, dummy.matrix);
      meshes[v].setColorAt(i, TONES[r.tone]);
      counts[v]++;
    }
    for (let v = 0; v < VARIANTS; v++) {
      meshes[v].count = hulls[v].count = counts[v];
      meshes[v].instanceMatrix.needsUpdate = hulls[v].instanceMatrix.needsUpdate = true;
      meshes[v].instanceColor.needsUpdate = true;
    }
    for (const x of objects) x.o.visible = intersects(x.box, rect);
  }
  return { sync, stats: () => ({ rocks: meshes.reduce((n, m) => n + m.count, 0) }) };
}
