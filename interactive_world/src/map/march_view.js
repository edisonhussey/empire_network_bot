// Marches, windowed: the nearest few are animated squads walking their drawn route (retimed so their gait matches the ground speed) with an
// orange arrow for what is left of it; the rest are plain dots. Position is always progress x route length, i.e. a fraction of the duration.
import * as THREE from 'three';
import { CELL, UNIT_SCALE, LOD } from '../world/config.js';
import { RouteArrow } from './arrow.js';

const BASE_SPEED = 0.2;      // ground speed (modelled units/s) the March clip was built for, at scale 1; a squad drawn at UNIT_SCALE walks UNIT_SCALE x faster (m/s)
const FIGHT_SECONDS = 4;
const SPAWNS_PER_SYNC = 2;   // route searches are spread over frames

export function createMarchView({ scene, world, router, template, clips, skins, teams }) {
  const active = new Map();   // march id -> detailed instance
  const fighting = [];
  const dots = new THREE.InstancedMesh(new THREE.ConeGeometry(4, 11, 6).translate(0, 5.5, 0), new THREE.MeshBasicMaterial({ color: '#ffffff', toneMapped: false }), 1024);
  dots.frustumCulled = false; dots.count = 0;
  dots.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(1024 * 3), 3);
  scene.add(dots);
  let dotMarches = [];

  function spawn(march) {
    const route = router.route(march.from, march.to);
    const root = new THREE.Group();
    const model = template.clone(true);
    root.add(model);
    root.scale.setScalar(UNIT_SCALE);
    skins.apply(root, teams[world.teamOf(march)] ?? teams.blue);
    scene.add(root);
    const mixer = new THREE.AnimationMixer(model);
    const actions = Object.fromEntries(clips.map((c) => [c.name, mixer.clipAction(c)]));
    const speed = (route.length * CELL) / march.duration;   // scene units per second along the drawn route
    actions.March.play().setEffectiveTimeScale(THREE.MathUtils.clamp(speed / (BASE_SPEED * UNIT_SCALE), 0.4, 3));
    mixer.setTime(Math.random() * 0.8);
    const arrow = new RouteArrow();
    scene.add(arrow.group);
    const first = route.polyline.pointAt(world.progress(march) * route.length);
    root.position.set(first.x * CELL, world.walkHeightAt(first.x, first.y), first.y * CELL);
    active.set(march.id, { march, route, root, mixer, actions, arrow, heading: first.heading });
  }

  const release = (m) => {
    scene.remove(m.root);
    scene.remove(m.arrow.group);
    m.arrow.dispose();
    m.mixer.stopAllAction();
  };

  world.on('arrive', (march) => {
    const m = active.get(march.id);
    if (!m) return;
    active.delete(march.id);
    m.arrow.hide();
    m.actions.March.stop();
    m.actions.Attack.reset().play();
    const end = m.route.polyline.pointAt(m.route.length);
    m.root.position.set(end.x * CELL, world.walkHeightAt(end.x, end.y), end.y * CELL);
    fighting.push({ ...m, left: FIGHT_SECONDS });
  });

  /** marches: those inside the window; focus: node the camera looks at. */
  function sync(marches, focus) {
    const ordered = [...marches].sort((a, b) => (a.from.x - focus.x) ** 2 + (a.from.y - focus.y) ** 2 - ((b.from.x - focus.x) ** 2 + (b.from.y - focus.y) ** 2));
    const detailed = new Set(ordered.slice(0, LOD.maxArmies).map((m) => m.id));
    for (const [id, m] of active) if (!detailed.has(id)) { release(m); active.delete(id); }
    let spawned = 0;
    for (const m of ordered) {
      if (!detailed.has(m.id) || active.has(m.id)) continue;
      if (spawned++ >= SPAWNS_PER_SYNC) break;
      spawn(m);
    }
    dotMarches = ordered.filter((m) => !active.has(m.id));
  }

  const dummy = new THREE.Object3D();
  const color = new THREE.Color();
  return {
    sync,
    pendingSpawns: () => [...active.keys()].length,
    update(dt) {
      for (const m of active.values()) {
        const p = m.route.polyline.pointAt(world.progress(m.march) * m.route.length);
        m.root.position.set(p.x * CELL, world.walkHeightAt(p.x, p.y), p.y * CELL);
        let diff = p.heading - m.heading;
        diff = Math.atan2(Math.sin(diff), Math.cos(diff));
        m.heading += diff * Math.min(1, dt * 7);   // turn smoothly at corners
        m.root.rotation.y = m.heading;
        m.mixer.update(dt);
        m.arrow.set(m.route.polyline.remaining(world.progress(m.march) * m.route.length), (x, y) => world.walkHeightAt(x, y));
        m.arrow.update(dt);
      }
      for (let i = fighting.length - 1; i >= 0; i--) {
        const f = fighting[i];
        f.mixer.update(dt);
        f.left -= dt;
        if (f.left <= 0) { release(f); fighting.splice(i, 1); }
      }
      let n = 0;
      for (const m of dotMarches) {
        if (n >= 1024) break;
        const p = world.positionOf(m);
        dummy.position.set(p.x * CELL, world.walkHeightAt(p.x, p.y) + 0.8, p.y * CELL);
        dummy.updateMatrix();
        dots.setMatrixAt(n, dummy.matrix);
        dots.setColorAt(n, color.set(world.kingdoms.get(m.kingdomId) ? '#ff8a1f' : '#ffffff'));
        n++;
      }
      dots.count = n;
      dots.instanceMatrix.needsUpdate = true;
      if (dots.instanceColor) dots.instanceColor.needsUpdate = true;
    },
    stats: () => ({ armies: active.size, dots: dotMarches.length }),
  };
}
