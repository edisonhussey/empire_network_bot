// Placed pieces (trees, rocks, buildings, bridges, fences, fields...) drawn with instancing: one InstancedMesh per piece and material, refilled
// for the render window only. Heights come from the land, so everything sits on the ground. Pieces flagged `outline` get a cartoon hull too.
import * as THREE from 'three';
import { CELL, TREE } from '../world/config.js';
import { hullGeometry } from '../render/outline.js';

const CAP = { tree: 7000, tuft: 12000, other: 800 };
const COVER_RADIUS = 42;   // grass tufts are only drawn this close (in nodes) to where the camera looks

export function createPropsView({ scene, world, library, outlineMat }) {
  const group = new THREE.Group();
  scene.add(group);
  const slots = new Map();   // `${asset}#${part}` -> { mesh, hull, cap, n }
  const dummy = new THREE.Object3D();
  const color = new THREE.Color();

  /** The instanced mesh (and cartoon hull) for part `i` of `parts`; `key` names the slot, `asset` supplies the capacity class and outline flag. */
  function slot(key, asset, parts, i) {
    key = `${key}#${i}`;
    let s = slots.get(key);
    if (!s) {
      const cap = asset.name.startsWith('tuft') ? CAP.tuft : /^(pine|oak|bush)/.test(asset.name) ? CAP.tree : CAP.other;
      const part = parts[i];
      const mesh = new THREE.InstancedMesh(part.geometry, part.material, cap);
      mesh.castShadow = mesh.receiveShadow = true;
      mesh.frustumCulled = false;
      mesh.count = 0;
      group.add(mesh);
      let hull = null;
      if (asset.outline && !part.material.transparent && part.material.side !== THREE.DoubleSide) {
        hull = new THREE.InstancedMesh(hullGeometry(part.geometry), outlineMat, cap);
        hull.frustumCulled = false; hull.count = 0;
        group.add(hull);
      }
      s = { mesh, hull, cap, n: 0 };
      slots.set(key, s);
    }
    return s;
  }

  /** Append one instance of every part to its slot. `asset.scale` ([x, y, z]) fits the piece to metres; p.sx/sy/sz are the field's own stretch on top. */
  function place(p, asset, parts, key) {
    const k = asset.scale ?? [1, 1, 1];
    dummy.position.set(p.x * CELL, p.h, p.y * CELL);
    dummy.rotation.set(0, p.rot, 0);
    dummy.scale.set(p.sx * k[0], p.sy * k[1], p.sz * k[2]);
    dummy.updateMatrix();
    color.setScalar(p.tint);
    for (let i = 0; i < parts.length; i++) {
      const s = slot(key, asset, parts, i);
      if (s.n >= s.cap) continue;
      s.mesh.setMatrixAt(s.n, dummy.matrix);
      s.mesh.setColorAt(s.n, color);
      s.hull?.setMatrixAt(s.n, dummy.matrix);
      s.n++;
    }
  }

  /** Redraw everything inside `rect` (nodes); `focus` is the node the camera looks at. */
  function sync(rect, focus = null) {
    const field = world.field;
    for (const s of slots.values()) s.n = 0;
    let total = 0;
    if (field) {
      const lod = [];   // pieces with a far version (pines): the nearest get the full model, the nearest few also their ground cover, the rest the cheap cones
      for (const p of field.propsIn(rect)) {
        const asset = library.get(p.asset);
        if (!asset) continue;
        if (focus && p.asset.startsWith('tuft') && (p.x - focus.x) ** 2 + (p.y - focus.y) ** 2 > COVER_RADIUS * COVER_RADIUS) continue;
        if (p.h === undefined) p.h = p.bridge ? 0 : field.heightAt(p.x, p.y);
        if (asset.far) { lod.push({ p, asset, d: focus ? (p.x - focus.x) ** 2 + (p.y - focus.y) ** 2 : Infinity }); continue; }
        place(p, asset, asset.parts, asset.name);
        total++;
      }
      lod.sort((a, b) => a.d - b.d);
      lod.forEach(({ p, asset, d }, i) => {
        const near = i < TREE.detailCap && d <= TREE.detailRadius ** 2;
        place(p, asset, near ? asset.parts : asset.far, `${asset.slotKey}${near ? '' : '_far'}`);
        if (near && i < TREE.extrasCap && d <= TREE.extrasRadius ** 2 && asset.extras.length) place(p, asset, asset.extras, `${asset.slotKey}_extras`);
        total++;
      });
    }
    for (const s of slots.values()) {
      s.mesh.count = s.n;
      s.mesh.instanceMatrix.needsUpdate = true;
      if (s.mesh.instanceColor) s.mesh.instanceColor.needsUpdate = true;
      if (s.hull) { s.hull.count = s.n; s.hull.instanceMatrix.needsUpdate = true; }
    }
    return total;
  }
  return { sync, stats: () => ({ props: [...slots.values()].reduce((n, s) => n + s.n, 0) }) };
}
