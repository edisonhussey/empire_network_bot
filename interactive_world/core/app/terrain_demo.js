// The terrain prototype page: generate the demonstration world, render it, and let you inspect it (views, outlines, connector highlight,
// wireframe, hover a cell to read its four corner heights). URL options for scripted renders:
//   core/?seed=7&view=wide|cells|cliffs|terraces|scarp&relief=1|2|4&outlines=0|1&highlight=1&wire=1&hover=cx,cz&ui=0
import * as THREE from 'three';
import { createStage } from '../render/stage.js';
import { createTerrainView } from '../render/terrain_view.js';
import { generateWorld, ZONE_NAMES } from '../terrain/generate.js';
import { CORNERS } from '../terrain/grid.js';
import { heightAt } from '../terrain/surface.js';

const params = new URLSearchParams(location.search);
const container = document.getElementById('app');
const panel = document.getElementById('panel');
if (params.get('ui') === '0') panel.hidden = true;

let seed = +params.get('seed') || 7;
let t0 = performance.now();
let world = generateWorld({ seed });
const genMs = performance.now() - t0;
const { grid } = world;
const W = grid.width * grid.cellSize, D = grid.depth * grid.cellSize;
const stage = createStage(container, { center: new THREE.Vector3(W / 2, 4, D / 2), extent: Math.max(W, D) });
const view = createTerrainView(stage.scene, grid);

// ---- camera presets, aimed at the generated features --------------------------------------------------------------------------------------------
const feature = (name) => world.features.find((f) => f.name === name);
// camera at feature + (dx, dy, dz) looking at feature + (ox, 0, oz)
const at = (f, dx, dy, dz, ty = null, ox = 0, oz = 0) => { const x = f.x + ox, z = f.z + oz, y = ty ?? heightAt(grid, x, z) ?? 0; return [[x + dx, y + dy, z + dz], [x, y, z]]; };
const VIEWS = {
  wide: () => [[-W * 0.12, W * 0.62, D * 1.32], [W / 2, 0, D / 2 - 20]],
  cells: () => at(feature('pad in the east lowland'), -22, 16, 34, null, -14, 20),
  cliffs: () => at(feature('mesa ramp'), -70, 48, 85),
  terraces: () => at(feature('terraced hill'), -40, 95, 175),
  scarp: () => at(feature('escarpment'), 30, 38, 95),
};
function setView(name) {
  const v = (VIEWS[name] ?? VIEWS.wide)();
  stage.look(v[0], v[1]);
  for (const b of panel.querySelectorAll('[data-view]')) b.classList.toggle('on', b.dataset.view === name);
}

// ---- hover: which cell is under the mouse, and its four corners ---------------------------------------------------------------------------------
const hover = new THREE.LineLoop(new THREE.BufferGeometry(), new THREE.LineBasicMaterial({ color: '#000000', depthTest: false }));
hover.renderOrder = 10; hover.visible = false;
stage.scene.add(hover);
function showCell(cx, cz) {
  if (!grid.inBounds(cx, cz)) { hover.visible = false; panel.querySelector('[data-cell]').innerHTML = '<span class="dim">hover the terrain</span>'; return; }
  const s = grid.surface(cx, cz), pts = [];
  for (let k = 0; k < 4; k++) { const [x, z] = grid.cornerXZ(cx, cz, k); pts.push(x, s[k] + 0.08, z); }
  hover.geometry.dispose();
  hover.geometry = new THREE.BufferGeometry().setAttribute('position', new THREE.Float32BufferAttribute(pts, 3));
  hover.visible = true;
  const zone = ZONE_NAMES[world.zones[grid.index(cx, cz)]];
  panel.querySelector('[data-cell]').innerHTML = `<b>cell (${cx}, ${cz})</b> · ${zone} · spread ${grid.spread(cx, cz).toFixed(2)} m<br>` +
    `<code>[${Array.from(s).map((h) => h.toFixed(2)).join(', ')}]</code><br><span class="dim">${CORNERS.join(', ')}</span>`;
}
const ray = new THREE.Raycaster(), ndc = new THREE.Vector2();
let queued = false;
stage.renderer.domElement.addEventListener('pointermove', (e) => {
  if (queued || e.buttons) return;
  queued = true;
  requestAnimationFrame(() => {
    queued = false;
    const r = stage.renderer.domElement.getBoundingClientRect();
    ndc.set(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1);
    ray.setFromCamera(ndc, stage.camera);
    const hit = ray.intersectObjects(view.topMeshes(), false)[0];
    if (!hit) return showCell(-1, -1);
    const r0 = hit.object.userData.range, i = hit.faceIndex >> 1, w = r0.x1 - r0.x0;   // two triangles per cell, cells row by row
    showCell(r0.x0 + (i % w), r0.z0 + Math.floor(i / w));
  });
});

// ---- panel ---------------------------------------------------------------------------------------------------------------------------------------
function paintStats() {
  const s = view.stats, w = world.stats;
  panel.querySelector('[data-stats]').innerHTML =
    `${grid.width} × ${grid.depth} cells of ${grid.cellSize} m (${W} × ${D} m) · seed ${seed}<br>` +
    `top ${s.top.toLocaleString()} tris · connectors ${s.connectors.toLocaleString()} tris on ${s.edges.toLocaleString()} edges · skirt ${s.skirt} · ${s.chunks} chunks<br>` +
    `cells flat ${w.flat} · spread ≤ 0.2 m ${(w.withinSpecShare * 100).toFixed(1)}% · max ${w.maxSpread} m<br>` +
    `generate ${genMs.toFixed(0)} ms · meshes ${s.ms.toFixed(0)} ms`;
}
const toggle = (name, fn, on) => { const b = panel.querySelector(`[data-t="${name}"]`); b.classList.toggle('on', on); fn(on); b.onclick = () => { const v = !b.classList.contains('on'); b.classList.toggle('on', v); fn(v); }; };
let outlinesOn = params.get('outlines') !== '0';
toggle('outlines', (v) => { outlinesOn = v; view.setOutlines(v); }, outlinesOn);
toggle('highlight', (v) => view.setHighlightConnectors(v), params.get('highlight') === '1');
toggle('wire', (v) => view.setWireframe(v), params.get('wire') === '1');
toggle('shadows', (v) => { stage.sun.castShadow = v; }, params.get('shadows') !== '0');
for (const b of panel.querySelectorAll('[data-view]')) b.onclick = () => setView(b.dataset.view);
function setRelief(k) { view.setRelief(k); hover.scale.y = k; for (const b of panel.querySelectorAll('[data-relief]')) b.classList.toggle('on', +b.dataset.relief === k); }
for (const b of panel.querySelectorAll('[data-relief]')) b.onclick = () => setRelief(+b.dataset.relief);
setRelief(+params.get('relief') || 1);
panel.querySelector('[data-seed]').value = seed;
panel.querySelector('[data-seed]').onchange = (e) => { const u = new URL(location.href); u.searchParams.set('seed', e.target.value); location.href = u; };
paintStats();
setView(params.get('view') ?? 'wide');
if (params.has('hover')) { const [cx, cz] = params.get('hover').split(',').map(Number); showCell(cx, cz); } else showCell(-1, -1);

// cell outlines fade out with distance (10,000 cells of lines turn into moiré from far away)
stage.renderer.setAnimationLoop(() => {
  stage.controls.update();
  const d = stage.camera.position.distanceTo(stage.controls.target);
  view.setOutlineOpacity(Math.max(0, Math.min(0.4, 0.4 * (1 - (d - 120) / 260))));
  stage.render();
});
window.terrain = { world, grid, view, stage, setView, showCell };
window.__ready = true;
