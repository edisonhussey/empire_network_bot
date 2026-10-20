// The tile terrain page (core/tiles.html): the example scene on 1 m tiles with 0.2 m steps, dressed with the terrain kit
// (assets/development/terrain_kit.glb), the Fire-Kingdom Shrine on the plateau pad, a N/E/S/W compass, and a hover readout per tile.
// URL options for scripted renders:
//   core/tiles.html?view=hero|shrine|falls|ramp|rise|forms|top|wide&grid=1&props=0&wire=1&shadows=0&compass=0&footprint=1&ui=0&seed=3&hover=tx,tz
import * as THREE from 'three';
import { createStage } from '../render/stage.js';
import { createCompass } from '../render/compass.js';
import { createAssetView, loadAsset } from '../render/asset_view.js';
import { loadKit, createTileView } from '../render/tile_view.js';
import { buildExampleScene, distToPolyline } from '../tiles/scene_example.js';
import { classify, shapeCounts, tileHeightAt, ROTATION_NAMES } from '../tiles/tile_shapes.js';
import { GROUND_NAMES, STEP, CORNERS } from '../tiles/tile_grid.js';
import { dressGrid } from '../tiles/dress.js';
import { edgeStats } from '../tiles/tile_edges.js';
import { placeOnCells } from '../placement/footprint.js';

const KIT = '../assets/development/terrain_kit.glb';
const SHRINE_URL = '../assets/development/fire_shrine.glb';
const params = new URLSearchParams(location.search);
const container = document.getElementById('app');
const panel = document.getElementById('panel');
if (params.get('ui') === '0') panel.hidden = true;

const scene0 = buildExampleScene();
const { grid } = scene0;
const W = grid.width, D = grid.depth;
const stage = createStage(container, { center: new THREE.Vector3(W / 2, 2, D / 2), extent: 64 });

// ---- daylight: warm sun from the south-west, blue sky fill, soft haze ------------------------------------------------------------------------------
{
  const sky = new THREE.Color('#b3d0ea');
  const c = document.createElement('canvas'); c.width = 4; c.height = 256;
  const g = c.getContext('2d'), grad = g.createLinearGradient(0, 0, 0, 256);
  grad.addColorStop(0, '#5f97d1'); grad.addColorStop(0.55, '#a9cbe9'); grad.addColorStop(1, '#e8eef0');
  g.fillStyle = grad; g.fillRect(0, 0, 4, 256);
  const bg = new THREE.CanvasTexture(c); bg.colorSpace = THREE.SRGBColorSpace;
  stage.scene.background = bg;
  stage.scene.fog = new THREE.Fog(sky, 90, 220);
  stage.scene.children.filter((o) => o.isHemisphereLight).forEach((h) => { h.color.set('#dcebff'); h.groundColor.set('#6b7442'); h.intensity = 1.25; });
  stage.sun.color.set('#fff0d8');
  stage.sun.intensity = 2.9;
  stage.sun.position.set(W / 2 - 38, 46, D / 2 + 30);
  stage.sun.target.position.set(W / 2, 0, D / 2);
  stage.sun.shadow.normalBias = 0.04;
  stage.sun.shadow.bias = -0.0004;
  Object.assign(stage.sun.shadow.camera, { left: -42, right: 42, top: 42, bottom: -42, near: 1, far: 160 });
  stage.sun.shadow.camera.updateProjectionMatrix();
  stage.renderer.toneMappingExposure = 1.05;
}
const compass = createCompass(stage.scene, container, { width: W, depth: D, height: 6, pad: 5, size: 3.5 });

// ---- views -------------------------------------------------------------------------------------------------------------------------------------------
const feature = (name) => scene0.features.find((f) => f.name === name);
const at = (f, dx, dy, dz, lift = 0) => { const y = (tileHeightAt(grid, f.x, f.z) ?? 0) + lift; return [[f.x + dx, y + dy, f.z + dz], [f.x, y, f.z]]; };
const VIEWS = {
  hero: () => [[-20, 40, 74], [W / 2 + 1, 0, D / 2 + 2]],
  wide: () => [[W / 2, 62, D + 50], [W / 2, 0, D / 2]],
  top: () => [[W / 2, 78, D / 2 + 0.01], [W / 2, 0, D / 2]],
  shrine: () => at(feature('shrine pad'), -26, 14, 14, 3),
  falls: () => at(feature('waterfall'), 9, 6, 13, 0.5),
  ramp: () => at(feature('lower ramp'), 13, 9, 12, 0.5),
  rise: () => at(feature('rocky rise'), -14, 9, 16),
  forms: () => at(feature('ridge and saddles'), -9, 8, 11),
};
function setView(name) {
  const v = (VIEWS[name] ?? VIEWS.hero)();
  stage.look(v[0], v[1]);
  for (const b of panel.querySelectorAll('[data-view]')) b.classList.toggle('on', b.dataset.view === name);
}

// ---- build ---------------------------------------------------------------------------------------------------------------------------------------------
let view = null, shrine = null, shrineInfo = null;
const assets = createAssetView(stage.scene);
const seed = +params.get('seed') || 3;
const dressing = dressGrid(grid, {
  seed, region: scene0.region, ramps: scene0.ramps, distToPath: (x, z) => distToPolyline(scene0.path, x, z),
  exclude: [scene0.padRect],
});

const ready = (async () => {
  const kit = await loadKit(KIT);
  view = createTileView(stage.scene, grid, kit, dressing);
  const sh = scene0.shrine;
  const p = placeOnCells({ cx: sh.tx, cz: sh.tz, w: sh.w, d: sh.d, facing: sh.facing, cellSize: 1 });
  try {
    shrineInfo = await loadAsset(SHRINE_URL);
    shrine = await assets.place(SHRINE_URL, p, grid.cornerHeight(sh.tx, sh.tz, 0), { cellSize: 1 });
  } catch (e) { console.warn('shrine not loaded', e); }
  panel.querySelector('[data-asset]').innerHTML =
    `<b>terrain_kit.glb</b> · ${kit.pieces.size} pieces (5 tiles + ${kit.pieces.size - 5} dressing)<br>` +
    (shrine ? `<b>fire_shrine.glb</b> faces ${p.facing} on the plateau pad · tiles x ${p.rect.x0}–${p.rect.x1 - 1}, z ${p.rect.z0}–${p.rect.z1 - 1} · ${shrineInfo.triangles.toLocaleString()} tris` : '<span class="dim">shrine not loaded</span>');
  wireToggles();
  paintStats();
})();

// ---- hover: tile shape, rotation, corners ---------------------------------------------------------------------------------------------------------
const hover = new THREE.LineLoop(new THREE.BufferGeometry(), new THREE.LineBasicMaterial({ color: '#111111', depthTest: false }));
hover.renderOrder = 10; hover.visible = false;
stage.scene.add(hover);
function showTile(tx, tz) {
  const box = panel.querySelector('[data-cell]');
  if (!grid.inBounds(tx, tz)) { hover.visible = false; box.innerHTML = '<span class="dim">hover the terrain</span>'; return; }
  const c = grid.corners(tx, tz), pts = [];
  for (let k = 0; k < 4; k++) { const [x, z] = grid.cornerXZ(tx, tz, k); pts.push(x, c[k] * STEP + 0.04, z); }
  hover.geometry.dispose();
  hover.geometry = new THREE.BufferGeometry().setAttribute('position', new THREE.Float32BufferAttribute(pts, 3));
  hover.visible = true;
  const { shape, rotation, base } = classify(c), i = grid.index(tx, tz);
  box.innerHTML = `<b>tile (${tx}, ${tz})</b> · <b>${shape}</b>${shape === 'flat' ? '' : ` turned ${ROTATION_NAMES[rotation]}`} · ${GROUND_NAMES[grid.ground[i]]} · ${scene0.region[i]}<br>` +
    `levels <code>[${Array.from(c).join(', ')}]</code> = <code>[${Array.from(c).map((l) => (l * STEP).toFixed(1)).join(', ')}] m</code> · base ${(base * STEP).toFixed(1)} m` +
    (grid.waterHeight(tx, tz) !== null ? ` · water ${grid.waterHeight(tx, tz).toFixed(1)} m` : '') + `<br><span class="dim">${CORNERS.join(', ')}</span>`;
}
const ray = new THREE.Raycaster(), ndc = new THREE.Vector2();
let queued = false;
stage.renderer.domElement.addEventListener('pointermove', (e) => {
  if (queued || e.buttons || !view) return;
  queued = true;
  requestAnimationFrame(() => {
    queued = false;
    const r = stage.renderer.domElement.getBoundingClientRect();
    ndc.set(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1);
    ray.setFromCamera(ndc, stage.camera);
    const hit = ray.intersectObjects(view.topMeshes(), false)[0];
    if (!hit) return showTile(-1, -1);
    const ti = hit.object.userData.tileOfTri[hit.faceIndex];
    showTile(ti % W, Math.floor(ti / W));
  });
});

// ---- panel -----------------------------------------------------------------------------------------------------------------------------------------
function paintStats() {
  const s = view.stats, sc = shapeCounts(grid), es = edgeStats(dressing.edges);
  const info = stage.renderer.info.render;
  panel.querySelector('[data-stats]').innerHTML =
    `${W} × ${D} tiles of 1 m (${W * D} tiles = ${(W / 5) * (D / 5)} cells of 5 m) · 0.2 m levels<br>` +
    `shapes: flat ${sc.flat} · corner high ${sc.corner_high} · corner low ${sc.corner_low} · ramp ${sc.ramp} · saddle ${sc.saddle}<br>` +
    `edges: ${es.cliff} cliff (tallest ${es.tallest.toFixed(1)} m) · ${es.riser} riser · ${dressing.falls.length} waterfall<br>` +
    `tops ${s.tops.toLocaleString()} tris · walls ${s.walls.toLocaleString()} · water ${s.water}<br>` +
    `pieces: ${s.instances.toLocaleString()} instances in ${s.pieces} instanced meshes · ${Math.round(s.pieceTris).toLocaleString()} tris<br>` +
    `last frame: ${info.calls} draw calls · ${info.triangles.toLocaleString()} tris · build ${s.ms.toFixed(0)} ms`;
}
function wireToggles() {
  const toggle = (name, fn, on) => { const b = panel.querySelector(`[data-t="${name}"]`); b.classList.toggle('on', on); fn(on); b.onclick = () => { const v = !b.classList.contains('on'); b.classList.toggle('on', v); fn(v); }; };
  toggle('grid', (v) => view.setGrid(v), params.get('grid') === '1');
  toggle('props', (v) => view.setProps(v), params.get('props') !== '0');
  toggle('wire', (v) => view.setWireframe(v), params.get('wire') === '1');
  toggle('shadows', (v) => { stage.sun.castShadow = v; }, params.get('shadows') !== '0');
  toggle('compass', (v) => compass.setVisible(v), params.get('compass') !== '0');
  toggle('footprint', (v) => assets.setOutlines(v), params.get('footprint') === '1');
}
for (const b of panel.querySelectorAll('[data-view]')) b.onclick = () => setView(b.dataset.view);
setView(params.get('view') ?? 'hero');
if (params.has('hover')) { const [tx, tz] = params.get('hover').split(',').map(Number); showTile(tx, tz); } else showTile(-1, -1);

let frames = 0;
stage.renderer.setAnimationLoop(() => {
  stage.controls.update();
  const t = performance.now() / 1000;
  view?.tick(t);
  assets.tick(t);
  compass.update(stage.camera, stage.controls.target);
  stage.render();
  if (view && ++frames % 60 === 2) paintStats();
});
window.tiles = { scene: scene0, grid, dressing, stage, setView, showTile, get view() { return view; }, assets };
ready.then(() => { window.__ready = true; }, (e) => { console.error(e); window.__ready = true; });
