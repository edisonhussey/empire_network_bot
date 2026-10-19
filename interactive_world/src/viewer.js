import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js';
import { makeOutlineMat, setOutlineWidth, decorate } from './render/outline.js';
import { TeamSkins, TEAMS } from './world/teams.js';
import { loadArmy, HOME } from './army.js';

const params = new URLSearchParams(location.search);
const MODEL = params.get('model') || 'assets/castle.glb';
if (params.has('shot')) document.body.classList.add('shot');
const $ = (id) => document.getElementById(id);

// ---- model switcher: the castle, the tower and the map library groups (assets/preview/*.glb, written by build_map_library.mjs)
for (const [label, file] of [['Castle', 'assets/castle.glb'], ['Tower', 'assets/tower.glb'], ['Pine', 'assets/pine.glb'], ['Trees', 'assets/preview/trees.glb'], ['Stone', 'assets/preview/stone.glb'], ['Terrain', 'assets/preview/terrain.glb'], ['Farm', 'assets/preview/farm.glb'], ['Buildings', 'assets/preview/buildings.glb'], ['Crossings', 'assets/preview/crossings.glb']]) {
  const b = document.createElement('button');
  b.textContent = label;
  b.classList.toggle('on', MODEL === file);
  b.onclick = () => { const q = new URLSearchParams(location.search); q.set('model', file); location.search = q.toString(); };
  $('models').append(b);
}

// ---- renderer, scene, camera ----------------------------------------------------------------------------------------------------------
const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
renderer.setSize(innerWidth, innerHeight);
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.shadowMap.enabled = true;
renderer.shadowMap.type = THREE.PCFSoftShadowMap;
document.body.prepend(renderer.domElement);

const scene = new THREE.Scene();
const paper = new THREE.Color('#ece6d8');
scene.background = paper;
scene.fog = new THREE.Fog(paper, 18, 42);
const pmrem = new THREE.PMREMGenerator(renderer);
const envMap = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
scene.environment = envMap;

const camera = new THREE.PerspectiveCamera(22, innerWidth / innerHeight, 0.1, 200);
const controls = new OrbitControls(camera, renderer.domElement);
controls.enableDamping = true;
controls.dampingFactor = 0.08;
controls.maxPolarAngle = Math.PI * 0.495;
controls.minDistance = 1.5;
controls.maxDistance = 30;

// ---- lights ---------------------------------------------------------------------------------------------------------------------------
const hemi = new THREE.HemisphereLight('#cfe3ff', '#8c7a60', 1.0);
scene.add(hemi);
const sun = new THREE.DirectionalLight('#fff1d6', 3);
sun.castShadow = true;
sun.shadow.mapSize.set(4096, 4096);
sun.shadow.camera.left = -4; sun.shadow.camera.right = 4; sun.shadow.camera.top = 4; sun.shadow.camera.bottom = -4;
sun.shadow.camera.near = 1; sun.shadow.camera.far = 30;
sun.shadow.bias = -0.0004;
sun.shadow.normalBias = 0.02;
sun.shadow.radius = 4;
scene.add(sun, sun.target);

const LIGHTS = {
  Day:      { az: -35, el: 48, sun: 2.2, fill: 0.8, env: 0.3, exp: 0.9, sunColor: '#fff1d6', sky: '#cfe3ff', bg: '#ece6d8' },
  Golden:   { az: -70, el: 16, sun: 3.6, fill: 0.7, env: 0.35, exp: 1.0, sunColor: '#ffb868', sky: '#ffd9a8', bg: '#e8d6bb' },
  Overcast: { az: -20, el: 60, sun: 0.7, fill: 1.5, env: 0.8, exp: 1.1, sunColor: '#e8eef5', sky: '#dfe6ee', bg: '#dfe3e6' },
  Dusk:     { az: 120, el: 10, sun: 1.6, fill: 0.5, env: 0.28, exp: 0.9, sunColor: '#ff8f5a', sky: '#7f93c4', bg: '#b9b2c4' },
  Studio:   { az: -45, el: 40, sun: 2.2, fill: 1.2, env: 0.9, exp: 1.0, sunColor: '#ffffff', sky: '#ffffff', bg: '#d8d8d8' },
};
const state = { light: 'Day', outline: true, shadows: true, wire: false, ground: true, flat: false, spin: false };

function applyLight(name, keepAngles = false) {
  const L = LIGHTS[name];
  state.light = name;
  if (!keepAngles) { $('az').value = L.az; $('el').value = L.el; }
  $('sun').value = L.sun; $('fill').value = L.fill; $('env').value = L.env; $('exp').value = L.exp;
  sun.color.set(L.sunColor);
  hemi.color.set(L.sky);
  paper.set(L.bg);
  scene.fog.color.set(L.bg);
  for (const b of document.querySelectorAll('#lights button')) b.classList.toggle('on', b.textContent === name);
  syncLight();
}
function syncLight() {
  const az = (+$('az').value * Math.PI) / 180, el = (+$('el').value * Math.PI) / 180;
  const d = 12;
  sun.position.set(Math.sin(az) * Math.cos(el) * d, Math.sin(el) * d, Math.cos(az) * Math.cos(el) * d);
  sun.target.position.set(0, 0.8, 0);
  sun.intensity = +$('sun').value;
  hemi.intensity = +$('fill').value;
  scene.environmentIntensity = +$('env').value;
  renderer.toneMappingExposure = +$('exp').value;
  for (const [id, f] of [['az', 0], ['el', 0], ['sun', 2], ['fill', 2], ['env', 2], ['exp', 2]]) $(id + 'Out').textContent = (+$(id).value).toFixed(f);
}
for (const id of ['az', 'el', 'sun', 'fill', 'env', 'exp']) $(id).addEventListener('input', syncLight);
for (const name of Object.keys(LIGHTS)) {
  const b = document.createElement('button');
  b.textContent = name;
  b.onclick = () => applyLight(name);
  $('lights').append(b);
}

// ---- ground ---------------------------------------------------------------------------------------------------------------------------
const ground = new THREE.Mesh(
  new THREE.CircleGeometry(40, 64),
  new THREE.MeshStandardMaterial({ color: '#d8d1bf', roughness: 1, metalness: 0 })
);
ground.rotation.x = -Math.PI / 2;
ground.receiveShadow = true;
scene.add(ground);

// ---- model ----------------------------------------------------------------------------------------------------------------------------
const group = new THREE.Group();
scene.add(group);
let triangles = 0;
const center = new THREE.Vector3(0, 1.0, 0);
let radius = 4;

const castleOutline = makeOutlineMat(1);
// the viewer's team: editable colours, applied to the castle flag and the squad (see world/teams.js)
const skins = new TeamSkins();
const team = { ...TEAMS.blue, id: 'viewer' };
const paintTeams = () => { skins.apply(group, team); skins.apply(army.root, team); skins.refresh(team); };

new GLTFLoader().load(MODEL, (gltf) => {
  const t0 = performance.now();
  const model = gltf.scene;
  const box = new THREE.Box3().setFromObject(model);
  const size = box.getSize(new THREE.Vector3());
  const c = box.getCenter(new THREE.Vector3());
  // centre on the ground plane: footprint centre at the origin, lowest point at y = 0
  model.position.set(-c.x, -box.min.y, -c.z);
  group.add(model);
  box.setFromObject(group);
  box.getCenter(center);
  radius = box.getSize(new THREE.Vector3()).length() / 2;
  triangles += decorate(model, castleOutline);
  skins.apply(model, team);
  $('stats').textContent = `${Math.round(triangles).toLocaleString()} triangles · ${size.x.toFixed(2)} × ${size.z.toFixed(2)} × ${size.y.toFixed(2)} units`;
  $('msg').remove();
  goView('Iso', true);
  applyLight(params.get('light') && LIGHTS[params.get('light')] ? params.get('light') : 'Day');
  applyLook();
  if (params.has('view')) goView(params.get('view'), true);
  window.__ready = true;
  console.log('loaded in', Math.round(performance.now() - t0), 'ms');
}, undefined, (e) => { $('msg').textContent = 'Could not load ' + MODEL + ' (' + (e?.message || e) + ')'; });

// ---- army ------------------------------------------------------------------------------------------------------------------------------
// ?army=March&t=0.3&pause freezes a pose for inspection; ?noarmy hides the squad.
const armyOutline = makeOutlineMat(0.4);
const clock = new THREE.Clock();
const army = loadArmy({
  scene, GLTFLoader, decorate: (root, mat) => { const n = decorate(root, mat); skins.apply(root, team); return n; }, outlineMat: armyOutline,
  onReady: (a) => {
    a.show(!params.has('noarmy'));
    for (const name of a.clips) {
      const b = document.createElement('button');
      b.textContent = name;
      b.onclick = () => { a.stopSequence(); a.play(name); for (const x of $('clips').children) x.classList.toggle('on', x === b); };
      $('clips').append(b);
    }
    const start = params.get('army') && a.clips.includes(params.get('army')) ? params.get('army') : 'Idle';
    a.play(start, 0);
    for (const x of $('clips').children) x.classList.toggle('on', x.textContent === start);
    if (params.has('t')) a.mixer.update(+params.get('t'));
    if (params.has('pause')) a.paused = true;
    if (params.has('walk')) a.walking = true;
    if (params.has('assault')) a.assault();
    window.__armyReady = true;
  },
});
for (const t of Object.values(TEAMS)) {
  const b = document.createElement('button');
  b.textContent = t.name;
  b.onclick = () => { team.primary = t.primary; team.secondary = t.secondary; $('colA').value = t.primary; $('colB').value = t.secondary; paintTeams(); };
  $('teams').append(b);
}
$('colA').value = team.primary; $('colB').value = team.secondary;
$('colA').oninput = () => { team.primary = $('colA').value; paintTeams(); };
$('colB').oninput = () => { team.secondary = $('colB').value; paintTeams(); };
$('armyShow').onclick = () => { army.show(!army.root.visible); $('armyShow').classList.toggle('on', army.root.visible); };
$('armyWalk').onclick = () => { army.stopSequence(); army.walking = !army.walking; $('armyWalk').classList.toggle('on', army.walking); if (army.walking) { army.play('March'); for (const x of $('clips').children) x.classList.toggle('on', x.textContent === 'March'); } };
$('armyAssault').onclick = () => { army.assault(); $('armyShow').classList.add('on'); };
$('armyPause').onclick = () => { army.paused = !army.paused; $('armyPause').classList.toggle('on', army.paused); };

// ---- camera presets (the sheet's views) ------------------------------------------------------------------------------------------------
const VIEWS = {
  Iso:   { az: -32, el: 30, dist: 0.8 },
  Front: { az: 0, el: 3, dist: 0.95 },
  Right: { az: 90, el: 3, dist: 0.95 },
  Back:  { az: 180, el: 3, dist: 0.95 },
  Left:  { az: -90, el: 3, dist: 0.95 },
  Top:   { az: 0, el: 88, dist: 0.95 },
  Iso2:  { az: 40, el: 31, dist: 1.0 },
  Army:  { az: -38, el: 22, abs: 2.9, at: 'army' },    // close on the squad (follows it while it walks)
};
function goView(name, instant) {
  const v = VIEWS[name];
  if (!v) return;
  const az = (v.az * Math.PI) / 180, el = (v.el * Math.PI) / 180;
  // ?zoom=0.4&focus=x,y,z (offsets from the centre, in model units) frame a close-up: used to inspect details
  const zoom = +params.get('zoom') || 1;
  const focus = (params.get('focus') || '0,0,0').split(',').map(Number);
  const d = v.abs ? v.abs * zoom : (radius / Math.tan((camera.fov * Math.PI) / 360)) * v.dist * 1.05 * zoom;
  const home = v.at === 'army' ? new THREE.Vector3(army.root.position.x, 0.12, army.root.position.z) : center;
  controls.target.set(home.x + focus[0], home.y + focus[1], home.z + focus[2]);
  camera.position.set(controls.target.x + Math.sin(az) * Math.cos(el) * d, controls.target.y + Math.sin(el) * d, controls.target.z + Math.cos(az) * Math.cos(el) * d);
  controls.update();
  followArmy = v.at === 'army';
  armyLast.copy(army.root.position);
  for (const b of document.querySelectorAll('#views button')) b.classList.toggle('on', b.textContent === name);
}
let followArmy = false;
const armyLast = new THREE.Vector3();
for (const name of Object.keys(VIEWS)) {
  const b = document.createElement('button');
  b.textContent = name;
  b.onclick = () => goView(name);
  $('views').append(b);
}

// ---- look toggles ---------------------------------------------------------------------------------------------------------------------
function applyLook() {
  renderer.shadowMap.enabled = state.shadows;
  sun.castShadow = state.shadows;
  ground.visible = state.ground;
  setOutlineWidth(+$('olw').value);
  $('olwOut').textContent = (+$('olw').value).toFixed(4);
  group.traverse((o) => {
    if (o.name === 'outline') o.visible = state.outline;
    if (o.isMesh && o.userData.base) {
      const m = o.userData.base;
      m.wireframe = state.wire;
      m.needsUpdate = true;
    }
  });
  for (const [id, key] of [['tShadow', 'shadows'], ['tOutline', 'outline'], ['tWire', 'wire'], ['tGround', 'ground'], ['tFlat', 'flat'], ['spin', 'spin']]) $(id).classList.toggle('on', state[key]);
  scene.traverse((o) => { if (o.material) o.material.needsUpdate = true; });
}
for (const [id, key] of [['tShadow', 'shadows'], ['tOutline', 'outline'], ['tWire', 'wire'], ['tGround', 'ground'], ['spin', 'spin']]) {
  $(id).onclick = () => { state[key] = !state[key]; controls.autoRotate = state.spin; applyLook(); };
}
$('tFlat').onclick = () => {
  state.flat = !state.flat;
  hemi.intensity = state.flat ? 2.6 : +$('fill').value;
  sun.intensity = state.flat ? 0 : +$('sun').value;
  applyLook();
};
$('olw').addEventListener('input', applyLook);
controls.autoRotateSpeed = 1.2;
$('refBtn').onclick = () => { const r = $('ref'); r.style.display = r.style.display === 'block' ? 'none' : 'block'; $('refBtn').classList.toggle('on', r.style.display === 'block'); };
$('overlay').src = 'assets/ref/hero.png';
$('ovl').addEventListener('input', () => { $('overlay').style.opacity = $('ovl').value; $('ovlOut').textContent = (+$('ovl').value).toFixed(2); });
$('shot').onclick = () => { const a = document.createElement('a'); a.download = 'castle.png'; a.href = renderer.domElement.toDataURL('image/png'); a.click(); };
if (params.has('wire')) { state.wire = true; }
if (params.has('nooutline')) { state.outline = false; }
if (params.has('noshadow')) { state.shadows = false; }

addEventListener('resize', () => {
  camera.aspect = innerWidth / innerHeight;
  camera.updateProjectionMatrix();
  renderer.setSize(innerWidth, innerHeight);
});
renderer.setAnimationLoop(() => {
  army.update(Math.min(clock.getDelta(), 0.1));
  if (followArmy) { // the Army view rides along while the squad walks
    const dx = army.root.position.clone().sub(armyLast);
    camera.position.add(dx); controls.target.add(dx);
    armyLast.copy(army.root.position);
  }
  controls.update();
  renderer.render(scene, camera);
});
