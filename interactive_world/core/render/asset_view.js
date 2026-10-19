// Layer 3, building GLBs on the terrain: load once, clone per placement, put at a placeOnCells() result. Knows nothing about how the
// terrain is meshed; the caller passes the ground level (footprintLevel or heightAt). Protocol: architecture/ASSET_GLB_PROTOCOL.md.
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';

const loader = new GLTFLoader();
const templates = new Map();   // url -> Promise<{ scene, triangles }>

/** Load a GLB once; later calls share the same template. */
export function loadAsset(url) {
  if (!templates.has(url)) {
    templates.set(url, loader.loadAsync(url).then((gltf) => {
      let triangles = 0;
      gltf.scene.traverse((o) => {
        if (!o.isMesh) return;
        const g = o.geometry;
        triangles += (g.index ? g.index.count : g.attributes.position.count) / 3;
        const glow = /flame/i.test(o.material.name);
        o.castShadow = !glow;                 // flames are emissive; they should not cast shadows
        o.receiveShadow = !glow;
      });
      return { scene: gltf.scene, triangles, extras: gltf.scene.children[0]?.userData ?? {} };
    }));
  }
  return templates.get(url);
}

export function createAssetView(scene) {
  const group = new THREE.Group();
  group.name = 'assets';
  scene.add(group);
  const placed = [];

  /** Place a clone of `url` at placement { x, z, rotationY, rect } with its y = 0 at `level`. Returns the instance root. */
  async function place(url, placement, level, { cellSize = 5 } = {}) {
    const { scene: tpl } = await loadAsset(url);
    const root = new THREE.Group();
    const model = tpl.clone(true);
    root.add(model);
    root.position.set(placement.x, level, placement.z);
    root.rotation.y = placement.rotationY;
    root.userData = { url, placement, level, flames: [] };
    model.traverse((o) => { if (/^Shrine_Flame/.test(o.name) || /flame/i.test(o.name)) root.userData.flames.push({ o, s: o.scale.clone(), ph: Math.random() * 6.28 }); });
    root.add(footprintOutline(placement, cellSize));
    group.add(root);
    placed.push(root);
    return root;
  }

  /** Re-turn an instance in place (same rect origin cell), e.g. from the N/E/S/W buttons. */
  function move(root, placement) {
    root.position.x = placement.x;
    root.position.z = placement.z;
    root.rotation.y = placement.rotationY;
    root.userData.placement = placement;
    const old = root.getObjectByName('footprint');
    if (old) { root.remove(old); old.geometry.dispose(); }
    root.add(footprintOutline(placement));
  }

  /** Gentle flame flicker. */
  function tick(t) {
    for (const r of placed) for (const f of r.userData.flames) {
      const k = 1 + 0.06 * Math.sin(t * 7.3 + f.ph) + 0.04 * Math.sin(t * 13.1 + f.ph * 2);
      f.o.scale.set(f.s.x * (2 - k), f.s.y * k, f.s.z * (2 - k));
    }
  }

  function setOutlines(v) { for (const r of placed) { const o = r.getObjectByName('footprint'); if (o) o.visible = v; } }

  return { group, place, move, tick, setOutlines, placed };
}

/** The covered cell rect (drawn in the instance's local frame, so it turns with it) plus an arrow on the front edge. */
function footprintOutline(placement, cellSize = 5) {
  const { rect } = placement;
  const turned = placement.facing === 'E' || placement.facing === 'W';
  const hw = ((turned ? rect.z1 - rect.z0 : rect.x1 - rect.x0) * cellSize) / 2;   // half width/depth in the asset's own frame
  const hd = ((turned ? rect.x1 - rect.x0 : rect.z1 - rect.z0) * cellSize) / 2;
  const y = 0.06, pts = [];
  const seg = (a, b) => pts.push(a[0], y, a[1], b[0], y, b[1]);
  seg([-hw, -hd], [hw, -hd]); seg([hw, -hd], [hw, hd]); seg([hw, hd], [-hw, hd]); seg([-hw, hd], [-hw, -hd]);
  for (let x = -hw + cellSize; x < hw - 1e-6; x += cellSize) seg([x, -hd], [x, hd]);         // cell lines
  for (let z = -hd + cellSize; z < hd - 1e-6; z += cellSize) seg([-hw, z], [hw, z]);
  seg([0, -hd], [0, -hd - 3]); seg([0, -hd - 3], [-0.8, -hd - 2]); seg([0, -hd - 3], [0.8, -hd - 2]);   // front arrow (local -Z)
  const lines = new THREE.LineSegments(new THREE.BufferGeometry().setAttribute('position', new THREE.Float32BufferAttribute(pts, 3)),
    new THREE.LineBasicMaterial({ color: '#1e6bd6', depthTest: false, transparent: true, opacity: 0.85 }));
  lines.renderOrder = 9;
  lines.name = 'footprint';
  return lines;
}
