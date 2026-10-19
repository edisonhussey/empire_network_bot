// Grid lines at every node inside the window, plus an optional overlay of the nodes the pathfinder treats as blocked.
import * as THREE from 'three';
import { CELL, LEGACY_SCALE as S } from '../world/config.js';

export function createGridView({ scene, router, world }) {
  const group = new THREE.Group();
  const lines = new THREE.LineSegments(new THREE.BufferGeometry(), new THREE.LineBasicMaterial({ color: '#ffffff', transparent: true, opacity: 0.32, fog: false }));
  lines.frustumCulled = false;
  const blockedMat = new THREE.MeshBasicMaterial({ color: '#e0453a', transparent: true, opacity: 0.38, fog: false, depthWrite: false });
  const cap = 6000;
  const blocked = new THREE.InstancedMesh(new THREE.PlaneGeometry(CELL * 0.92, CELL * 0.92).rotateX(-Math.PI / 2), blockedMat, cap);
  blocked.frustumCulled = false;
  blocked.count = 0;
  group.add(lines, blocked);
  scene.add(group);
  const show = { grid: true, blocked: false };
  let last = null;

  function sync(rect) {
    last = rect;
    const lo = -world.pad, hi = world.size + world.pad;   // lines only over the playable map
    const x0 = Math.max(lo, Math.floor(rect.x0)), x1 = Math.min(hi, Math.ceil(rect.x1)), y0 = Math.max(lo, Math.floor(rect.y0)), y1 = Math.min(hi, Math.ceil(rect.y1));
    const pts = [];
    for (let x = x0; x <= x1; x++) pts.push(x * CELL, 0.03 * S, (y0 - 0.5) * CELL, x * CELL, 0.03 * S, (y1 + 0.5) * CELL);
    for (let y = y0; y <= y1; y++) pts.push((x0 - 0.5) * CELL, 0.03 * S, y * CELL, (x1 + 0.5) * CELL, 0.03 * S, y * CELL);
    lines.geometry.dispose();
    lines.geometry = new THREE.BufferGeometry();
    lines.geometry.setAttribute('position', new THREE.Float32BufferAttribute(pts, 3));
    if (show.blocked) {
      const nodes = router.walkGrid.blockedNodes(rect).slice(0, cap);
      const m = new THREE.Matrix4();
      nodes.forEach(([x, y], i) => blocked.setMatrixAt(i, m.makeTranslation(x * CELL, 0.05 * S, y * CELL)));
      blocked.count = nodes.length;
      blocked.instanceMatrix.needsUpdate = true;
    }
  }
  lines.visible = true;
  blocked.visible = false;
  return {
    sync,
    setGrid(v) { show.grid = v; lines.visible = v; },
    setBlocked(v) { show.blocked = v; blocked.visible = v; if (v && last) sync(last); },
  };
}
