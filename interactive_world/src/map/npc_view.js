// NPCs on the map (dungeons, the robber baron tower, ...), windowed. Each npc kind has a renderer in `renderers`; the default is a plain
// PLACEHOLDER marker. Register the real model for a kind with registerNpcRenderer(kind, (npc) => Object3D) when its art exists.
import * as THREE from 'three';
import { CELL, LOD } from '../world/config.js';
import { nameTag } from './labels.js';

export const renderers = {
  // placeholder: a dark tower stub, clearly not final art
  dungeon: () => {
    const g = new THREE.Group();
    const stone = new THREE.MeshStandardMaterial({ color: '#5b4a4a', roughness: 0.95, flatShading: true });
    const body = new THREE.Mesh(new THREE.CylinderGeometry(CELL * 0.8, CELL * 1.0, CELL * 2.2, 8), stone);
    body.position.y = CELL * 1.1;
    const roof = new THREE.Mesh(new THREE.ConeGeometry(CELL * 1.05, CELL * 1.1, 8), new THREE.MeshStandardMaterial({ color: '#7a2a2a', roughness: 0.8, flatShading: true }));
    roof.position.y = CELL * 2.75;
    for (const m of [body, roof]) m.castShadow = m.receiveShadow = true;
    g.add(body, roof);
    return g;
  },
};
export const registerNpcRenderer = (kind, factory) => { renderers[kind] = factory; };

export function createNpcView({ scene, world }) {
  const root = new THREE.Group();
  scene.add(root);
  const live = new Map();
  const pool = new Map(); // npcKind -> idle objects

  function sync(npcs, focus) {
    const ordered = npcs.length > LOD.maxNpcs ? [...npcs].sort((a, b) => (a.x - focus.x) ** 2 + (a.y - focus.y) ** 2 - ((b.x - focus.x) ** 2 + (b.y - focus.y) ** 2)).slice(0, LOD.maxNpcs) : npcs;
    const keep = new Set(ordered.map((n) => n.key));
    for (const [key, o] of live) if (!keep.has(key)) { o.visible = false; live.delete(key); (pool.get(o.userData.npcKind) ?? pool.set(o.userData.npcKind, []).get(o.userData.npcKind)).push(o); }
    for (const n of ordered) {
      let o = live.get(n.key);
      if (!o) {
        o = pool.get(n.npcKind)?.pop();
        if (!o) {
          o = new THREE.Group();
          o.add((renderers[n.npcKind] ?? renderers.dungeon)(n));
          const tag = nameTag(`${n.name}${renderers[n.npcKind] === renderers.dungeon ? ' (placeholder)' : ''}`);
          tag.position.set(0, 3.6 * CELL, 0);
          o.add(tag);
          o.userData.npcKind = n.npcKind;
          root.add(o);
        }
        live.set(n.key, o);
      }
      o.visible = true;
      o.position.set(n.x * CELL, world.heightAt(n.x, n.y), n.y * CELL);
    }
  }
  return { sync, stats: () => ({ npcs: live.size }) };
}
