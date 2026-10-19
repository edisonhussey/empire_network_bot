// The cartoon route arrow: a flat orange ribbon with an outline, flowing chevrons and a fat arrowhead, lying on the ground along a route.
// set(points) takes the part of the route still to walk (grid nodes); update(dt) animates the chevrons.
import * as THREE from 'three';
import { CELL } from '../world/config.js';
import { Polyline } from '../world/polyline.js';

// Metres. The arrow is an overlay, drawn wider than a 3 m path so it reads from the strategy camera.
const WIDTH = 3.1, HEAD_LEN = 7.5, HEAD_W = 8.5, OUTLINE = 0.7, PERIOD = 8, LIFT = 0.5;

function chevronTexture(base, stripe) {
  const c = document.createElement('canvas');
  c.width = 128; c.height = 64;
  const g = c.getContext('2d');
  g.fillStyle = base; g.fillRect(0, 0, 128, 64);
  g.fillStyle = stripe;
  g.beginPath(); g.moveTo(30, 6); g.lineTo(78, 32); g.lineTo(30, 58); g.lineTo(52, 58); g.lineTo(100, 32); g.lineTo(52, 6); g.closePath(); g.fill();
  const t = new THREE.CanvasTexture(c);
  t.wrapS = THREE.RepeatWrapping;
  t.colorSpace = THREE.SRGBColorSpace;
  return t;
}

/** Triangle-strip ribbon along `pts` (world xz pairs), `width` wide, uv.x in chevron periods. */
function ribbon(pts, width, y, hf) {
  const pos = [], uv = [], idx = [];
  let run = 0;
  for (let i = 0; i < pts.length; i++) {
    const prev = pts[Math.max(0, i - 1)], next = pts[Math.min(pts.length - 1, i + 1)];
    let dx = next.x - prev.x, dz = next.z - prev.z;
    const l = Math.hypot(dx, dz) || 1;
    dx /= l; dz /= l;
    // miter: widen at corners so the ribbon keeps its width
    const inD = i > 0 ? [pts[i].x - pts[i - 1].x, pts[i].z - pts[i - 1].z] : [dx, dz];
    const il = Math.hypot(inD[0], inD[1]) || 1;
    const cos = Math.max(0.55, (inD[0] / il) * dx + (inD[1] / il) * dz);
    const k = width / 2 / cos;
    if (i > 0) run += Math.hypot(pts[i].x - pts[i - 1].x, pts[i].z - pts[i - 1].z);
    const ya = y + (hf ? hf(pts[i].x - dz * k, pts[i].z + dx * k) : 0), yb = y + (hf ? hf(pts[i].x + dz * k, pts[i].z - dx * k) : 0);
    pos.push(pts[i].x - dz * k, ya, pts[i].z + dx * k, pts[i].x + dz * k, yb, pts[i].z - dx * k);
    uv.push(run / PERIOD, 0, run / PERIOD, 1);
    if (i < pts.length - 1) idx.push(i * 2, i * 2 + 1, i * 2 + 2, i * 2 + 1, i * 2 + 3, i * 2 + 2);
  }
  const g = new THREE.BufferGeometry();
  g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
  g.setAttribute('uv', new THREE.Float32BufferAttribute(uv, 2));
  g.setIndex(idx);
  return g;
}

function head(tip, dir, len, w, y, hf) {
  const n = { x: -dir.z, z: dir.x };
  const bx = tip.x - dir.x * len, bz = tip.z - dir.z * len;
  const g = new THREE.BufferGeometry();
  const H = (x, z) => y + (hf ? hf(x, z) : 0);
  g.setAttribute('position', new THREE.Float32BufferAttribute([bx + n.x * w / 2, H(bx + n.x * w / 2, bz + n.z * w / 2), bz + n.z * w / 2, bx - n.x * w / 2, H(bx - n.x * w / 2, bz - n.z * w / 2), bz - n.z * w / 2, tip.x, H(tip.x, tip.z), tip.z], 3));
  g.setIndex([0, 1, 2]);
  return g;
}

/** Insert points so no segment is longer than `step` (the arrow then follows the terrain's height). */
function resample(pts, step) {
  if (!isFinite(step)) return pts;
  const out = [pts[0]];
  for (let i = 1; i < pts.length; i++) {
    const a = pts[i - 1], b = pts[i], n = Math.ceil(Math.hypot(b.x - a.x, b.y - a.y) / step);
    for (let k = 1; k <= n; k++) out.push({ x: a.x + ((b.x - a.x) * k) / n, y: a.y + ((b.y - a.y) * k) / n });
  }
  return out;
}

export class RouteArrow {
  constructor({ color = '#ff7a00', stripe = '#ffc24d', edge = '#7a3200', opacity = 1 } = {}) {
    this.group = new THREE.Group();
    this.group.visible = false;
    this.texture = chevronTexture(color, stripe);
    const flat = { transparent: true, opacity, depthWrite: false, side: THREE.DoubleSide, toneMapped: false, fog: false, polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2 };
    this.mats = {
      body: new THREE.MeshBasicMaterial({ map: this.texture, ...flat }),
      head: new THREE.MeshBasicMaterial({ color, ...flat }),
      edge: new THREE.MeshBasicMaterial({ color: edge, ...flat }),
    };
    this.parts = { edgeBody: new THREE.Mesh(), edgeHead: new THREE.Mesh(), body: new THREE.Mesh(), head: new THREE.Mesh() };
    this.parts.edgeBody.material = this.parts.edgeHead.material = this.mats.edge;
    this.parts.body.material = this.mats.body;
    this.parts.head.material = this.mats.head;
    for (const m of Object.values(this.parts)) { m.frustumCulled = false; m.renderOrder = 5; this.group.add(m); }
    this.parts.edgeBody.renderOrder = this.parts.edgeHead.renderOrder = 4;
  }

  setOpacity(o) {
    for (const m of Object.values(this.mats)) m.opacity = o;
  }

  /** points: route nodes still to walk, [{x, y}] in grid nodes. Hides the arrow when there is nothing left to show. */
  set(points, heightAt = null) {
    if (!points || points.length < 2) { this.group.visible = false; return; }
    const hf = heightAt ? (x, z) => heightAt(x / CELL, z / CELL) : null;
    const world = resample(points.map((p) => ({ x: p.x * CELL, y: p.y * CELL })), hf ? 3 : Infinity);
    const poly = new Polyline(world);
    const L = poly.length;
    if (L < 1) { this.group.visible = false; return; }
    const hl = Math.min(HEAD_LEN, L * 0.8);
    const bodyEnd = L - hl;
    const tipP = world[world.length - 1];
    const baseP = poly.pointAt(bodyEnd);
    const dir = { x: tipP.x - baseP.x, z: tipP.y - baseP.y };
    const dl = Math.hypot(dir.x, dir.z) || 1;
    dir.x /= dl; dir.z /= dl;
    const body = [{ x: world[0].x, z: world[0].y }];
    for (let i = 1; i < world.length; i++) if (poly.cum[i] < bodyEnd - 1e-6) body.push({ x: world[i].x, z: world[i].y });
    body.push({ x: baseP.x, z: baseP.y });
    const tip = { x: tipP.x, z: tipP.y };
    const swap = (mesh, geo) => { mesh.geometry?.dispose(); mesh.geometry = geo; };
    swap(this.parts.edgeBody, ribbon(body, WIDTH + OUTLINE * 2, LIFT - 0.1, hf));
    swap(this.parts.body, ribbon(body, WIDTH, LIFT, hf));
    swap(this.parts.edgeHead, head({ x: tip.x + dir.x * OUTLINE * 1.6, z: tip.z + dir.z * OUTLINE * 1.6 }, dir, hl + OUTLINE * 2.4, HEAD_W + OUTLINE * 2.4, LIFT - 0.1, hf));
    swap(this.parts.head, head(tip, dir, hl, HEAD_W, LIFT + 0.03, hf));
    this.group.visible = true;
  }

  hide() {
    this.group.visible = false;
  }

  update(dt) {
    this.texture.offset.x -= dt * 0.9;   // chevrons flow towards the arrowhead
  }

  dispose() {
    for (const m of Object.values(this.parts)) m.geometry?.dispose();
    for (const m of Object.values(this.mats)) m.dispose();
    this.texture.dispose();
  }
}
