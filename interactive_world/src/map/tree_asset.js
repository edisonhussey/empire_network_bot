// The pine from its own model (assets/pine.glb, made by tools/build_tree.mjs; 1 scene unit = 1 m) instead of being baked into map_library.glb.
// The field asks for `pine`; this module answers with a library-shaped asset: `parts` (needles and bark), `extras` (the pine's own pebbles, grass tufts and
// gold sparkles, drawn only on the nearest trees) and `far`, a cheap stack of cones built from the model's measured tiers for everything further away.
import * as THREE from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import { TREE } from '../world/config.js';

/** Library-shaped assets: name -> { name, slotKey, parts, extras, far, scale: [x, y, z], outline }. */
export async function loadPines(loader, url) {
  const gltf = await loader.loadAsync(url);
  const info = gltf.parser.json.extras;
  const byName = new Map();
  gltf.scene.traverse((o) => {
    if (!o.isMesh) return;
    o.material.envMapIntensity = 1;
    byName.set(o.name, { geometry: o.geometry, material: o.material });
  });
  const pick = (names) => names.map((n) => byName.get(n)).filter(Boolean);
  const k = TREE.height / info.height_m;
  const pine = { name: 'pine', slotKey: 'pine', group: 'trees', outline: false, parts: pick(info.body), extras: pick(info.extras), far: farParts(info), scale: [k, k, k], footprint: [1, 1] };
  return new Map([[pine.name, pine]]);
}

/** Cone per needle tier plus a trunk, one geometry with vertex colours: ~130 triangles for the whole tree. */
function farParts({ tiers }) {
  const leaf = new THREE.Color(TREE.farLeaf), dark = leaf.clone().multiplyScalar(0.72), trunk = new THREE.Color(TREE.farTrunk);
  const pieces = [];
  const add = (g, color) => {
    g = g.index ? g.toNonIndexed() : g;
    g.deleteAttribute('uv');
    const c = new Float32Array(g.attributes.position.count * 3);
    for (let i = 0; i < c.length; i += 3) { c[i] = color.r; c[i + 1] = color.g; c[i + 2] = color.b; }
    g.setAttribute('color', new THREE.BufferAttribute(c, 3));
    pieces.push(g);
  };
  const trunkH = tiers[tiers.length - 1].y0;
  add(new THREE.CylinderGeometry(0.35, 0.8, trunkH, 6).translate(0, trunkH / 2, 0), trunk);
  tiers.forEach((t, i) => {
    const h = t.y1 - t.y0;
    add(new THREE.ConeGeometry(t.r * 0.9, h, 8, 1, true).translate(0, t.y0 + h / 2, 0), i % 2 ? dark : leaf);
  });
  const geometry = mergeGeometries(pieces);
  geometry.computeVertexNormals();
  const material = new THREE.MeshStandardMaterial({ vertexColors: true, flatShading: true, roughness: 0.9, side: THREE.DoubleSide });
  return [{ geometry, material }];
}
