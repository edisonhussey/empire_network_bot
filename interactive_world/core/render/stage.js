// Layer 4, rendering shell: renderer, camera, orbit controls and soft neutral lighting for inspecting geometry. Knows nothing about terrain.
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

export function createStage(container, { center = new THREE.Vector3(250, 4, 250), extent = 500 } = {}) {
  const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.0;
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;
  container.prepend(renderer.domElement);
  renderer.domElement.style.cssText = 'display:block;width:100%;height:100%';

  const scene = new THREE.Scene();
  const bg = new THREE.Color('#d7d8da');
  scene.background = bg;
  scene.fog = new THREE.Fog(bg, extent * 1.4, extent * 3.2);

  const camera = new THREE.PerspectiveCamera(35, 1, 0.5, extent * 8);
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.maxPolarAngle = Math.PI * 0.49;
  controls.target.copy(center);

  // soft light: a strong grey sky/ground fill and one gentle sun with soft shadows, so slopes and walls separate by value only
  scene.add(new THREE.HemisphereLight('#f2f3f5', '#3c3e42', 0.8));
  const sun = new THREE.DirectionalLight('#ffffff', 2.3);
  sun.position.set(center.x - extent * 0.7, extent * 0.55, center.z + extent * 0.45);   // ~33 degrees up: low enough to model gentle slopes
  sun.target.position.copy(center);
  sun.castShadow = true;
  sun.shadow.mapSize.set(4096, 4096);
  const R = extent * 0.62;
  Object.assign(sun.shadow.camera, { left: -R, right: R, top: R, bottom: -R, near: 1, far: extent * 3 });
  sun.shadow.bias = -0.0002;
  sun.shadow.normalBias = 0.4;
  sun.shadow.radius = 4;
  scene.add(sun, sun.target);

  function resize() {
    const w = container.clientWidth || innerWidth, h = container.clientHeight || innerHeight;
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    renderer.setSize(w, h, false);
  }
  new ResizeObserver(resize).observe(container);
  resize();

  /** Put the camera at `position` looking at `target` (both [x, y, z]). */
  function look(position, target) {
    controls.target.set(...target);
    camera.position.set(...position);
    controls.update();
  }

  return { THREE, renderer, scene, camera, controls, sun, look, render: () => renderer.render(scene, camera) };
}
