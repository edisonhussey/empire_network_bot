// The map app: mount it into any container with createMapApp({ container }) and drive it through the returned API. It composes the world
// (src/world: kingdoms, castles, the land, marches) with the stage and the views; each concern lives in its own module. Only what is inside
// the camera's window is ever instantiated: ground chunks, props, castles, squads.
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { createStage } from './stage.js';
import { createGridView } from './grid_view.js';
import { createTerrainView } from './terrain_view.js';
import { createGroundView } from './ground_view.js';
import { createWaterView } from './water_view.js';
import { createPropsView } from './props_view.js';
import { createCastleView } from './castle_view.js';
import { createNpcView, registerNpcRenderer } from './npc_view.js';
import { createMarchView } from './march_view.js';
import { createUI } from './ui.js';
import { RouteArrow } from './arrow.js';
import { loadLibrary } from './library.js';
import { loadPines } from './tree_asset.js';
import { makeOutlineMat, setOutlineWidth, decorate } from '../render/outline.js';
import { World } from '../world/world.js';
import { Router } from '../world/routing.js';
import { Focus } from '../world/focus.js';
import { loadScenario, SHEET, generated } from '../world/scenario.js';
import { TEAMS, TeamSkins } from '../world/teams.js';
import { CELL, WINDOW, BUILDING_HEIGHT } from '../world/config.js';
import { grow, same } from '../world/rect.js';

/**
 * options: container (required), assets ('assets/' base path of the .glb files), world (a World; default: the sheet map scenario),
 *          autoClock (advance the world clock with real time; set false to drive it with api.setNow from a server clock)
 */
export async function createMapApp({ container, assets = 'assets/', world = null, autoClock = true } = {}) {
  container.insertAdjacentHTML('beforeend', '<div class="mw-msg">Loading map…</div>');
  if (!world) { world = new World({ size: SHEET.size }); loadScenario(world, SHEET); }
  const teams = structuredClone(TEAMS);        // editable: change a colour and every castle and squad of that team follows
  const skins = new TeamSkins();
  const focus = new Focus(world);
  const router = new Router(world);

  // ---- assets -------------------------------------------------------------------------------------------------------------------------------
  const loader = new GLTFLoader();
  const [castleGltf, armyGltf, towerGltf, library, pines] = await Promise.all([
    loader.loadAsync(`${assets}castle.glb`), loader.loadAsync(`${assets}army.glb`), loader.loadAsync(`${assets}tower.glb`),
    loadLibrary(loader, `${assets}map_library.glb`, { skins, team: teams.baron }),
    loadPines(loader, `${assets}pine.glb`),      // pines live in their own model, in metres; they join the library under the names the field asks for
  ]);
  for (const [name, asset] of pines) library.set(name, asset);
  setOutlineWidth(0.0035);
  const wrap = (gltf, pivot = [0, 0]) => {      // pivot: the model's footprint centre (x, z); it is moved onto the map node
    const g = new THREE.Group();
    gltf.scene.position.set(-pivot[0], 0, -pivot[1]);
    g.add(gltf.scene);
    return g;
  };
  const fitHeight = (group, metres) => {         // scale a landmark so it stands `metres` tall (1 scene unit = 1 m)
    const box = new THREE.Box3().setFromObject(group);
    group.scale.setScalar(metres / (box.max.y - box.min.y));
  };
  const castleTemplate = wrap(castleGltf, castleGltf.parser.json.extras?.pivot ?? [1.25, 1.02]);
  const towerTemplate = wrap(towerGltf);
  fitHeight(castleTemplate, BUILDING_HEIGHT.castle);
  fitHeight(towerTemplate, BUILDING_HEIGHT.tower);
  decorate(castleTemplate, makeOutlineMat(1), { track: false });
  decorate(towerTemplate, makeOutlineMat(1), { track: false });
  decorate(armyGltf.scene, makeOutlineMat(0.4), { track: false });
  registerNpcRenderer('robber_baron', () => {   // the watchtower (24 m, its rock base fills most of the 3x3 footprint)
    const t = towerTemplate.clone(true);
    skins.apply(t, teams.baron);
    return t;
  });

  // ---- views --------------------------------------------------------------------------------------------------------------------------------
  const stage = createStage({ container, world });
  const propOutline = makeOutlineMat(1);
  const gridView = createGridView({ scene: stage.scene, router, world });
  const groundView = createGroundView({ scene: stage.scene, world });
  const waterView = createWaterView({ scene: stage.scene, world });
  const propsView = createPropsView({ scene: stage.scene, world, library, outlineMat: propOutline });
  const terrainView = createTerrainView({ scene: stage.scene, world, outlineMat: makeOutlineMat(1.2) });
  const castleView = createCastleView({ scene: stage.scene, world, template: castleTemplate, skins, teams });
  const npcView = createNpcView({ scene: stage.scene, world });
  const marchView = createMarchView({ scene: stage.scene, world, router, template: armyGltf.scene, clips: armyGltf.animations, skins, teams });
  const preview = new RouteArrow({ opacity: 0.6 });
  stage.scene.add(preview.group);

  // ---- actions the UI and the public API share ------------------------------------------------------------------------------------------------------
  let override = null, lastRect = null, lastVersion = -1, dirty = true, acc = 0, rect = null;
  const api = {
    world, focus, teams, stage,
    selectKingdom: (id) => focus.selectKingdom(id),
    /** Add (or replace) a kingdom from data { id, name, team, castles: [[x, y], ...], npc: [[x, y], ...] } and optionally select it. */
    loadKingdom(data, { select = true } = {}) {
      world.addKingdom(data);
      dirty = true;
      if (select) focus.selectKingdom(data.id);
    },
    removeKingdom(id) { world.removeKingdom(id); if (focus.kingdomId === id) focus.selectKingdom(null); dirty = true; },
    /** Replace the land with a MapField spec (rivers, roads, forests, buildings...). Castles stay where they are. */
    setMapSpec(spec) { world.setField({ size: world.size, ...spec }); dirty = true; },
    /** { from: [x, y], to: [x, y], duration (seconds), startedAt?, kingdomId? }; the route is only drawn, `duration` decides the arrival. */
    sendAttack({ from, to, duration, startedAt, kingdomId }) {
      const f = Array.isArray(from) ? { x: from[0], y: from[1] } : from;
      const owner = kingdomId ?? world.entityAt(f.x, f.y)?.kingdomId;
      return world.sendMarch({ from, to, duration, startedAt, kingdomId: owner });
    },
    setNow: (t) => world.setNow(t),
    /** Pin the render window (nodes {x0,y0,x1,y1}); null returns to following the camera. */
    setWindow(rect_) { override = rect_; dirty = true; },
    frame: (r, seconds) => stage.frame(r, seconds),
    /** Run the clock and the animation forward without rendering (scripted starts, screenshots). */
    simulate(seconds, step = 0.05) {
      syncMarches();
      for (let t = 0; t < seconds; t += step) { world.advance(step); if (Math.floor(t / 0.25) !== Math.floor((t + step) / 0.25)) syncMarches(); marchView.update(step); }
    },
    /** Finish camera moves, rebuild what the window needs and build every queued ground chunk now (scripted starts, screenshots). */
    refresh() { stage.update(5); syncStatic(); syncMarches(); while (groundView.pending()) groundView.update(); },
    dispose() { stage.dispose(); container.classList.remove('mw'); },
  };

  const ui = createUI({
    container, world, teams, focus, stage, gridView,
    actions: {
      markSource: (e) => castleView.markSource(e),
      preview(from, to) {
        if (!from || !to) { preview.hide(); return; }
        preview.set(router.route({ x: from.x, y: from.y }, { x: to.x, y: to.y }).points, (x, y) => world.walkHeightAt(x, y));
      },
      sendAttack: (from, to, duration) => api.sendAttack({ from, to, duration }),
      teamsChanged(team) { skins.refresh(team); world.emit('team', {}); },
      setMap(kind, seed) { api.setMapSpec(kind === 'gen' ? generated(seed).map : SHEET.map); },
    },
  });
  container.querySelector('.mw-msg')?.remove();

  focus.on((id) => {
    dirty = true;
    const b = id && world.kingdomBounds(id);
    if (b) stage.frame(grow(b, 3));
  });

  // ---- picking ----------------------------------------------------------------------------------------------------------------------------------
  const canvas = stage.renderer.domElement;
  const pick = (e) => { const n = stage.pickNode(e.clientX, e.clientY); return n ? world.entityAt(n.x, n.y) : null; };
  let down = null, hoverQueued = false;
  canvas.addEventListener('pointerdown', (e) => { down = { x: e.clientX, y: e.clientY }; });
  canvas.addEventListener('pointerup', (e) => { if (down && Math.hypot(e.clientX - down.x, e.clientY - down.y) < 5) ui.click(pick(e)); down = null; });
  canvas.addEventListener('pointermove', (e) => {
    if (hoverQueued || e.buttons) return;
    hoverQueued = true;
    requestAnimationFrame(() => { hoverQueued = false; ui.hover(pick(e)); });
  });

  // ---- windowed sync: only what the camera can see is instantiated -------------------------------------------------------------------------------
  const currentRect = () => override ?? stage.groundRect(WINDOW.margin);
  const focusNode = () => ({ x: stage.controls.target.x / CELL, y: stage.controls.target.z / CELL });
  function syncMarches() { rect = currentRect(); marchView.sync(world.marchesInWindow(rect), focusNode()); }
  function syncStatic() {
    rect = currentRect();
    const { castles, npcs } = world.queryWindow(rect);
    const f = focusNode();
    groundView.sync(rect, f);
    waterView.sync(rect);
    const props = propsView.sync(rect, f);
    castleView.sync(castles, f, focus.kingdomId);
    npcView.sync(npcs, f);
    terrainView.sync(rect);
    gridView.sync(rect);
    lastRect = rect; lastVersion = world.version; dirty = false;
    const s = { ...castleView.stats(), ...terrainView.stats(), ...npcView.stats(), ...groundView.stats() };
    ui.setStats(`window ${Math.round(rect.x1 - rect.x0)}×${Math.round(rect.y1 - rect.y0)} nodes · ${castles.length} castles (${s.detailed} full, ${s.proxies} proxy) · ${npcs.length} npc · ${props} props · ${s.chunks} ground chunks · ${world.kingdoms.size} kingdoms of ${world.castles.size} castles`);
  }
  world.on('march', () => { acc = 1; });
  world.on('arrive', () => { acc = 1; });
  world.on('field', () => { dirty = true; });

  const clock = new THREE.Clock();
  stage.renderer.setAnimationLoop(() => {
    const dt = Math.min(clock.getDelta(), 0.1);
    if (autoClock) world.advance(dt);
    stage.update(dt);
    acc += dt;
    if (acc > 0.25) {
      acc = 0;
      if (dirty || world.version !== lastVersion || !same(currentRect(), lastRect, 1.5)) syncStatic();
      syncMarches();
    }
    groundView.update();
    waterView.update(dt);
    marchView.update(dt);
    preview.update(dt);
    ui.update(dt);
    stage.render();
  });

  stage.lookAtMap();
  syncStatic();
  api.ready = true;
  return api;
}
