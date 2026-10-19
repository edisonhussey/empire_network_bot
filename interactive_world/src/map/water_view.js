// Rivers: a translucent ribbon along each river's centreline with a slowly flowing texture, built in sections only where the window needs them.
import * as THREE from 'three';
import { CELL } from '../world/config.js';
const toNodes = (metres) => metres / CELL;
import { WATER_LEVEL } from '../world/mapfield/constants.js';

const SECTION = 10;   // nodes of river per mesh

function flowTexture() {
  const c = document.createElement('canvas');
  c.width = 128; c.height = 256;
  const g = c.getContext('2d');
  const grad = g.createLinearGradient(0, 0, 128, 0);
  grad.addColorStop(0, '#3f9fe0'); grad.addColorStop(0.5, '#2f8ad2'); grad.addColorStop(1, '#3f9fe0');
  g.fillStyle = grad; g.fillRect(0, 0, 128, 256);
  g.strokeStyle = 'rgba(255,255,255,.35)'; g.lineWidth = 3; g.lineCap = 'round';
  for (const [x, y, l] of [[30, 20, 60], [92, 100, 70], [48, 170, 50], [78, 225, 40]]) { g.beginPath(); g.moveTo(x, y); g.lineTo(x, y + l); g.stroke(); }
  g.strokeStyle = 'rgba(255,255,255,.7)'; g.lineWidth = 5;                      // a flow arrow like the sheet's
  g.beginPath(); g.moveTo(64, 70); g.lineTo(64, 150); g.moveTo(50, 132); g.lineTo(64, 152); g.lineTo(78, 132); g.stroke();
  const t = new THREE.CanvasTexture(c);
  t.wrapS = t.wrapT = THREE.RepeatWrapping;
  t.colorSpace = THREE.SRGBColorSpace;
  return t;
}

export function createWaterView({ scene, world }) {
  const group = new THREE.Group();
  scene.add(group);
  const texture = flowTexture();
  const material = new THREE.MeshStandardMaterial({ map: texture, vertexColors: true, roughness: 0.3, metalness: 0, transparent: true, opacity: 0.95, envMapIntensity: 0.9, depthWrite: false });
  const sections = new Map();   // `${river}:${i}` -> { mesh, box }
  let builtFor = -1;

  function build(river, i) {
    const path = river.path, s0 = i * SECTION, s1 = Math.min(path.length, (i + 1) * SECTION + 0.5);
    const inner = river.width / 2 - toNodes(0.5), half = river.width / 2 + toNodes(4);   // full strength inside the banks, fading to nothing under the bank
    const offs = [-half, -inner, inner, half], alpha = [0, 1, 1, 0], us = [0, 0.3, 0.7, 1];
    const pos = [], uv = [], rgba = [], idx = [];
    const n = Math.max(2, Math.ceil((s1 - s0) / 0.5));
    for (let k = 0; k <= n; k++) {
      const s = s0 + ((s1 - s0) * k) / n, p = path.pointAt(s);
      for (let c = 0; c < 4; c++) {
        pos.push((p.x - p.dy * offs[c]) * CELL, WATER_LEVEL, (p.y + p.dx * offs[c]) * CELL);
        uv.push(us[c], s / 3);
        rgba.push(1, 1, 1, alpha[c]);
      }
      if (k < n) for (let c = 0; c < 3; c++) { const a0 = k * 4 + c, b0 = a0 + 1, a1 = a0 + 4, b1 = a1 + 1; idx.push(a0, b0, a1, b0, b1, a1); }
    }
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
    g.setAttribute('uv', new THREE.Float32BufferAttribute(uv, 2));
    g.setAttribute('color', new THREE.Float32BufferAttribute(rgba, 4));
    g.setIndex(idx);
    g.computeVertexNormals();
    const mesh = new THREE.Mesh(g, material);
    mesh.receiveShadow = true;
    const a = path.pointAt(s0), b = path.pointAt(s1), m = half + 1;
    return { mesh, box: { x0: Math.min(a.x, b.x) - m - 4, y0: Math.min(a.y, b.y) - m - 4, x1: Math.max(a.x, b.x) + m + 4, y1: Math.max(a.y, b.y) + m + 4 } };
  }

  function sync(rect) {
    const field = world.field;
    if (builtFor !== world.version) {   // the land changed: drop every section
      for (const s of sections.values()) { group.remove(s.mesh); s.mesh.geometry.dispose(); }
      sections.clear();
      builtFor = world.version;
    }
    if (!field) return;
    field.rivers.forEach((river, ri) => {
      const count = Math.ceil(river.path.length / SECTION);
      for (let i = 0; i < count; i++) {
        const key = `${ri}:${i}`;
        let s = sections.get(key);
        if (!s) {
          const a = river.path.pointAt(i * SECTION), b = river.path.pointAt(Math.min(river.path.length, (i + 1) * SECTION));
          const pad = river.width + 6;
          if (Math.max(Math.min(a.x, b.x) - pad, rect.x0) > Math.min(Math.max(a.x, b.x) + pad, rect.x1) || Math.max(Math.min(a.y, b.y) - pad, rect.y0) > Math.min(Math.max(a.y, b.y) + pad, rect.y1)) continue;
          s = build(river, i);
          group.add(s.mesh);
          sections.set(key, s);
        }
        s.mesh.visible = !(s.box.x0 > rect.x1 || s.box.x1 < rect.x0 || s.box.y0 > rect.y1 || s.box.y1 < rect.y0);
      }
    });
  }
  return { sync, update(dt) { texture.offset.y -= dt * 0.12; }, stats: () => ({ water: sections.size }) };
}
