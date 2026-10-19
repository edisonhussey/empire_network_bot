// Cartoon outlines: an inverted hull (smooth-normal back-face copy of every mesh, pushed out along its normals).
// `k` scales the line width per asset family (soldiers are a tenth of the castle's size and need a thinner line).
import * as THREE from 'three';
import { mergeVertices } from 'three/addons/utils/BufferGeometryUtils.js';

const mats = [];
const hullCache = new Map();
let baseWidth = 0.0035;

export function makeOutlineMat(k = 1) {
  const m = new THREE.MeshBasicMaterial({ color: '#1b1511', side: THREE.BackSide });
  m.userData.k = k;
  m.userData.width = { value: baseWidth * k };
  m.onBeforeCompile = (shader) => {
    shader.uniforms.uWidth = m.userData.width;
    shader.vertexShader = shader.vertexShader.replace('#include <common>', '#include <common>\nuniform float uWidth;')
      .replace('#include <begin_vertex>', '#include <begin_vertex>\ntransformed += normal * uWidth;');
  };
  mats.push(m);
  return m;
}

export function setOutlineWidth(w) {
  baseWidth = w;
  for (const m of mats) m.userData.width.value = w * m.userData.k;
}

/** The smooth-normal hull geometry for a geometry (cached): used by decorate() and by instanced meshes that carry their own hull. */
export function hullGeometry(geometry) {
  let g = hullCache.get(geometry.uuid);
  if (!g) {
    g = geometry.clone();
    g.deleteAttribute('normal'); g.deleteAttribute('color'); g.deleteAttribute('uv');
    g = mergeVertices(g, 1e-4);
    g.computeVertexNormals();
    hullCache.set(geometry.uuid, g);
  }
  return g;
}

/** Shadows and outline hulls for every mesh under `root` (`track` also remembers each base material). Returns the triangle count. */
export function decorate(root, outlineMat, { track = true } = {}) {
  const meshes = [];
  root.traverse((o) => { if (o.isMesh) meshes.push(o); }); // collect first: adding the hulls while traversing would recurse into them
  let tris = 0;
  for (const o of meshes) {
    tris += o.geometry.attributes.position.count / 3;
    o.castShadow = true;
    o.receiveShadow = true;
    o.material.envMapIntensity = 1;
    if (track) o.userData.base = o.material; // the viewer's wireframe toggle needs it; clones of the mesh would copy it, so the map skips it
    if (o.material.side === THREE.DoubleSide) continue; // flat cloth: a hull would just be a second sheet
    const g = hullGeometry(o.geometry);
    const hull = new THREE.Mesh(g, outlineMat);
    hull.name = 'outline';
    hull.castShadow = false; hull.receiveShadow = false;
    o.add(hull);
  }
  return tris;
}
