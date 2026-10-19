// Castles, windowed and level-of-detail'd: the nearest few are full models (cloned, team-tinted, name-tagged, pooled); every other castle in
// the window is one instance of a cheap proxy. Selected-kingdom castles get a ring (instanced) and the attack source gets its own marker.
import * as THREE from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import { CELL, LOD } from '../world/config.js';
import { nameTag } from './labels.js';

const PROXY_CAP = 4096;

function proxyGeometry() {
  const keep = new THREE.BoxGeometry(CELL * 2.6, CELL * 1.2, CELL * 2.2).translate(0, CELL * 0.6, 0);
  const tower = new THREE.CylinderGeometry(CELL * 0.55, CELL * 0.62, CELL * 1.7, 10).translate(CELL * 0.9, CELL * 0.85, CELL * 0.4);
  const roof = new THREE.ConeGeometry(CELL * 0.72, CELL * 0.9, 10).translate(CELL * 0.9, CELL * 2.1, CELL * 0.4);
  return mergeGeometries([keep, tower, roof].map((g) => g.toNonIndexed()));
}

export function createCastleView({ scene, world, template, skins, teams }) {
  const root = new THREE.Group();
  scene.add(root);

  // full models, pooled: key -> { group }
  const live = new Map();
  const pool = [];
  const size = 3 * CELL;
  const make = () => {
    const group = new THREE.Group();
    group.add(template.clone(true));
    const tag = nameTag('');
    tag.position.set(0, 2.9 * CELL, 0);
    group.userData.tag = tag;
    group.add(tag);
    root.add(group);
    return group;
  };
  const retag = (group, text) => {
    if (group.userData.text === text) return;
    group.userData.text = text;
    const tag = nameTag(text);
    tag.position.copy(group.userData.tag.position);
    group.remove(group.userData.tag);
    group.userData.tag.material.map.dispose();
    group.userData.tag = tag;
    group.add(tag);
  };

  // proxies: one instanced mesh, coloured per team
  const proxy = new THREE.InstancedMesh(proxyGeometry(), new THREE.MeshStandardMaterial({ color: '#ffffff', roughness: 0.85, flatShading: true }), PROXY_CAP);
  proxy.castShadow = proxy.receiveShadow = true; proxy.frustumCulled = false; proxy.count = 0;
  proxy.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(PROXY_CAP * 3), 3);
  root.add(proxy);

  // rings: selected kingdom (instanced) and the attack source (single)
  const ringGeo = new THREE.RingGeometry(size * 0.74, size * 0.86, 40).rotateX(-Math.PI / 2);
  const ringMat = (c) => new THREE.MeshBasicMaterial({ color: c, transparent: true, opacity: 0.95, toneMapped: false, fog: false, depthWrite: false });
  const rings = new THREE.InstancedMesh(ringGeo, ringMat('#ffd54a'), PROXY_CAP);
  rings.frustumCulled = false; rings.count = 0;
  const source = new THREE.Mesh(ringGeo, ringMat('#ff8a1f'));
  source.scale.setScalar(1.18); source.position.y = 0.7; source.visible = false;
  root.add(rings, source);

  const dummy = new THREE.Object3D();
  const color = new THREE.Color();
  let last = { castles: [], focus: { x: 0, y: 0 }, kingdomId: null };

  /** castles: entities inside the window; focus: node the camera looks at (decides who gets a full model). */
  function sync(castles, focus, kingdomId) {
    last = { castles, focus, kingdomId };
    const ordered = castles.length > LOD.maxCastles ? [...castles].sort((a, b) => (a.x - focus.x) ** 2 + (a.y - focus.y) ** 2 - ((b.x - focus.x) ** 2 + (b.y - focus.y) ** 2)) : castles;
    const detailed = new Set(ordered.slice(0, LOD.maxCastles).map((c) => c.key));
    for (const [key, group] of live) if (!detailed.has(key)) { group.visible = false; live.delete(key); pool.push(group); }
    let np = 0, nr = 0;
    for (const c of ordered) {
      if (detailed.has(c.key)) {
        let group = live.get(c.key);
        if (!group) { group = pool.pop() ?? make(); live.set(c.key, group); }
        group.visible = true;
        group.position.set(c.x * CELL, world.heightAt(c.x, c.y), c.y * CELL);
        skins.apply(group, teams[world.teamOf(c)]);
        retag(group, c.name);
      } else if (np < PROXY_CAP) {
        dummy.position.set(c.x * CELL, world.heightAt(c.x, c.y), c.y * CELL);
        dummy.updateMatrix();
        proxy.setMatrixAt(np, dummy.matrix);
        proxy.setColorAt(np, color.set(teams[world.teamOf(c)].primary));
        np++;
      }
      if (c.kingdomId === kingdomId && nr < PROXY_CAP) {
        dummy.position.set(c.x * CELL, world.heightAt(c.x, c.y) + 0.6, c.y * CELL);
        dummy.updateMatrix();
        rings.setMatrixAt(nr++, dummy.matrix);
      }
    }
    proxy.count = np; rings.count = nr;
    proxy.instanceMatrix.needsUpdate = rings.instanceMatrix.needsUpdate = true;
    if (proxy.instanceColor) proxy.instanceColor.needsUpdate = true;
  }

  world.on('team', () => sync(last.castles, last.focus, last.kingdomId));

  return {
    sync,
    /** Mark the attack source (an entity) with the orange ring, or clear it with null. */
    markSource(e) {
      source.visible = !!e;
      if (e) source.position.set(e.x * CELL, world.heightAt(e.x, e.y) + 0.7, e.y * CELL);
    },
    stats: () => ({ detailed: live.size, proxies: proxy.count }),
  };
}
