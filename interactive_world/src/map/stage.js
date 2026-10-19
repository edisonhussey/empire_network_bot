// The stage: renderer inside a container, camera + orbit controls, lighting, ground. Also turns the camera into a window of the world
// (groundRect) and flies it to a rect (frame): the app renders only what that window contains.
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js';
import { CELL, WINDOW, LEGACY_SCALE as S } from '../world/config.js';
import { rectOf, grow, limit, center, size } from '../world/rect.js';

export const toScene = (x, y, h = 0) => new THREE.Vector3(x * CELL, h, y * CELL);

// Lighting presets (the panel's buttons). ambient = sky/ground fill, env = image-based fill, sun = direct light; warmth -1 (cool) .. 1 (golden);
// azimuth / elevation place the sun in degrees. Default is the look the map always had.
export const LIGHT_PRESETS = {
  Default: { ambient: 0.9, env: 0.3, sun: 2.4, warmth: 0, exposure: 0.9, azimuth: -40, elevation: 50, shadows: true },
  Ambient: { ambient: 1.9, env: 0.6, sun: 0.7, warmth: -0.15, exposure: 1.0, azimuth: -40, elevation: 62, shadows: true },   // soft, even, overcast-style fill
  Warm: { ambient: 0.75, env: 0.25, sun: 2.8, warmth: 0.85, exposure: 0.95, azimuth: -62, elevation: 24, shadows: true },    // low golden sun, long shadows
};
const lerpColor = (cool, mid, warm, w) => new THREE.Color(w < 0 ? cool : mid).lerp(new THREE.Color(w < 0 ? mid : warm), w < 0 ? w + 1 : w);
const TINT = {   // [cool, neutral, warm]
  sun: ['#dbe8ff', '#fff1d6', '#ffa95c'], sky: ['#bcd4ff', '#cfe3ff', '#ffd9a8'], ground: ['#7c8a8c', '#8c7a60', '#a8794a'], haze: ['#cfe0ea', '#d6e6cc', '#f1d9a8'],
};

const DIRS = { Iso: [-28, 40], Top: [0, 88], Side: [-90, 14] };   // [azimuth, elevation] degrees

export function createStage({ container, world }) {
  const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 0.9;
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;
  container.prepend(renderer.domElement);
  renderer.domElement.style.cssText = 'display:block;width:100%;height:100%';

  const skyColor = new THREE.Color('#d6e6cc');
  const scene = new THREE.Scene();
  scene.background = skyColor;
  scene.fog = new THREE.Fog(skyColor, 140 * S, 380 * S);
  scene.environment = new THREE.PMREMGenerator(renderer).fromScene(new RoomEnvironment(), 0.04).texture;
  scene.environmentIntensity = 0.3;

  const camera = new THREE.PerspectiveCamera(28, 1, 0.5 * S, 900 * S);
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.maxPolarAngle = Math.PI * 0.485;
  controls.minDistance = 4 * S;
  controls.maxDistance = 260 * S;

  const hemi = new THREE.HemisphereLight('#cfe3ff', '#8c7a60', 0.9);
  scene.add(hemi);
  const sun = new THREE.DirectionalLight('#fff1d6', 2.4);
  sun.castShadow = true;
  sun.shadow.mapSize.set(4096, 4096);
  const R = 56 * S;
  Object.assign(sun.shadow.camera, { left: -R, right: R, top: R, bottom: -R, near: 1, far: 220 * S * 2 });
  sun.shadow.bias = -0.00005;
  sun.shadow.normalBias = 0.03 * S;
  sun.shadow.radius = 3;
  scene.add(sun, sun.target);

  // ---- lighting: one parameter set (see LIGHT_PRESETS), applied to the lights, exposure, sky and fog --------------------------------------------------
  const light = { ...LIGHT_PRESETS.Default };
  const listeners = [];
  const sunDir = new THREE.Vector3();
  function applyLight() {
    const { ambient, env, warmth: w, exposure, azimuth, elevation, shadows } = light;
    hemi.intensity = ambient;
    hemi.color.copy(lerpColor(...TINT.sky, w)); hemi.groundColor.copy(lerpColor(...TINT.ground, w));
    scene.environmentIntensity = env;
    sun.intensity = light.sun;
    sun.color.copy(lerpColor(...TINT.sun, w));
    sun.castShadow = shadows;
    renderer.toneMappingExposure = exposure;
    skyColor.copy(lerpColor(...TINT.haze, w));   // the background uses this colour object; the fog keeps its own copy
    scene.fog.color.copy(skyColor);
    const a = (azimuth * Math.PI) / 180, e = (elevation * Math.PI) / 180;
    sunDir.set(Math.sin(a) * Math.cos(e), Math.sin(e), Math.cos(a) * Math.cos(e));
    listeners.forEach((fn) => fn(light));
  }
  const lighting = {
    params: light, presets: Object.keys(LIGHT_PRESETS),
    set(patch) { Object.assign(light, patch); applyLight(); },
    preset(name) { if (LIGHT_PRESETS[name]) { Object.assign(light, LIGHT_PRESETS[name]); applyLight(); } },
    presetValues: (name) => LIGHT_PRESETS[name],
    onChange(fn) { listeners.push(fn); },
  };
  applyLight();

  // horizon: a wide meadow far below the heightfield (the real ground is drawn in chunks by ground_view.js) that follows the camera target
  const meadow = new THREE.Mesh(new THREE.PlaneGeometry(1400 * S, 1400 * S), new THREE.MeshStandardMaterial({ color: '#6f9a3c', roughness: 1 }));
  meadow.rotation.x = -Math.PI / 2;
  meadow.receiveShadow = false;
  scene.add(meadow);

  function resize() {
    const w = container.clientWidth || innerWidth, h = container.clientHeight || innerHeight;
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    renderer.setSize(w, h, false);
  }
  const observer = new ResizeObserver(resize);
  observer.observe(container);
  resize();

  // ---- camera moves ---------------------------------------------------------------------------------------------------------------------------
  let tween = null;
  const fly = (target, position, seconds = 0.7) => {
    tween = seconds ? { t: 0, seconds, t0: controls.target.clone(), p0: camera.position.clone(), t1: target, p1: position } : null;
    if (!seconds) { controls.target.copy(target); camera.position.copy(position); controls.update(); }
  };

  /** Fly to look at a rect of nodes, keeping the current viewing direction. */
  function frame(rect, seconds = 0.7) {
    const c = center(rect), s = size(rect);
    const extent = Math.max(s.w, s.h) / 2 + 6;
    const dir = camera.position.clone().sub(controls.target);
    if (dir.lengthSq() < 1e-6) dir.set(-1, 1.2, 1.6);
    dir.normalize();
    const aspect = camera.aspect;
    const dist = Math.min(controls.maxDistance, ((extent * CELL) / Math.tan((camera.fov * Math.PI) / 360)) * 1.3 / Math.min(1, aspect));
    const target = toScene(c.x, c.y);
    fly(target, target.clone().add(dir.multiplyScalar(dist)), seconds);
  }

  /** Change the viewing direction (Iso / Top / Side) around the current target and distance. */
  function view(name, seconds = 0.5) {
    const [az, el] = DIRS[name] ?? DIRS.Iso;
    const d = Math.max(controls.minDistance, camera.position.distanceTo(controls.target)) || 130 * S;
    const a = (az * Math.PI) / 180, e = (el * Math.PI) / 180;
    const target = controls.target.clone();
    fly(target, target.clone().add(new THREE.Vector3(Math.sin(a) * Math.cos(e), Math.sin(e), Math.cos(a) * Math.cos(e)).multiplyScalar(d)), seconds);
  }

  // ---- the window: which nodes the camera can see -----------------------------------------------------------------------------------------------
  const ray = new THREE.Raycaster();
  const ndc = new THREE.Vector2();
  function groundRect(margin = WINDOW.margin) {
    camera.updateMatrixWorld(true);   // the camera may have been moved since the last render
    const pts = [];
    for (const [x, y] of [[-1, -1], [1, -1], [1, 1], [-1, 1]]) {
      ndc.set(x, y);
      ray.setFromCamera(ndc, camera);
      const { origin, direction } = ray.ray;
      const t = direction.y < -0.02 ? Math.min(-origin.y / direction.y, 500 * S) : 500 * S;   // rays above the horizon: cap the distance
      pts.push({ x: (origin.x + direction.x * t) / CELL, y: (origin.z + direction.z * t) / CELL });
    }
    const t = { x: controls.target.x / CELL, y: controls.target.z / CELL };
    return limit(grow(rectOf(pts), margin), t, WINDOW.maxRadius);
  }

  /** The node under a screen point on a horizontal plane `h` scene units up (castles are picked at mid-height). */
  const plane = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
  const hit = new THREE.Vector3();
  function pickNode(clientX, clientY, h = 10) {
    camera.updateMatrixWorld(true);
    const r = renderer.domElement.getBoundingClientRect();
    ndc.set(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
    ray.setFromCamera(ndc, camera);
    plane.constant = -h;
    return ray.ray.intersectPlane(plane, hit) ? { x: hit.x / CELL, y: hit.z / CELL } : null;
  }

  function update(dt) {
    if (tween) {
      tween.t += dt;
      const k = Math.min(1, tween.t / tween.seconds), e = k * k * (3 - 2 * k);
      controls.target.lerpVectors(tween.t0, tween.t1, e);
      camera.position.lerpVectors(tween.p0, tween.p1, e);
      if (k >= 1) tween = null;
    }
    controls.update();
    const t = controls.target;
    meadow.position.set(t.x, -0.9 * S, t.z);
    sun.position.set(t.x + sunDir.x * 700, sunDir.y * 700, t.z + sunDir.z * 700);
    sun.target.position.set(t.x, 0, t.z);
  }

  const lookAtMap = () => frame({ x0: 0, y0: 0, x1: world.size, y1: world.size }, 0);
  return {
    THREE, renderer, scene, camera, controls, lighting, frame, view, groundRect, pickNode, update, lookAtMap,
    render: () => renderer.render(scene, camera),
    dispose() { observer.disconnect(); renderer.setAnimationLoop(null); renderer.dispose(); renderer.domElement.remove(); },
  };
}
