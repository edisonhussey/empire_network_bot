// Layer 3, orientation aids: N / E / S / W letters standing at the map edges, and a compass rose in the corner of the screen that
// turns with the camera. North is -Z and east is +X (architecture/README.md, rule 1).
import * as THREE from 'three';

const LETTER_COLOR = { N: '#c62d1f', E: '#2b2b2b', S: '#2b2b2b', W: '#2b2b2b' };

function letterSprite(text, size) {
  const c = document.createElement('canvas');
  c.width = c.height = 128;
  const g = c.getContext('2d');
  g.fillStyle = 'rgba(255,255,255,0.85)';
  g.beginPath(); g.arc(64, 64, 58, 0, Math.PI * 2); g.fill();
  g.fillStyle = LETTER_COLOR[text];
  g.font = 'bold 84px -apple-system, Segoe UI, sans-serif';
  g.textAlign = 'center'; g.textBaseline = 'middle';
  g.fillText(text, 64, 70);
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  const s = new THREE.Sprite(new THREE.SpriteMaterial({ map: tex, depthTest: false, toneMapped: false }));
  s.scale.set(size, size, 1);
  s.renderOrder = 20;
  return s;
}

/**
 * @param {THREE.Scene} scene
 * @param {HTMLElement} container  the page element the rose is drawn in
 * @param {{ width: number, depth: number, height?: number, pad?: number, size?: number }} map  world size in metres; letters stand `pad` m outside the edges, `size` m tall
 */
export function createCompass(scene, container, { width, depth, height = 6, pad = 18, size = 16 }) {
  const group = new THREE.Group();
  group.name = 'compass';
  const at = { N: [width / 2, -pad], S: [width / 2, depth + pad], E: [width + pad, depth / 2], W: [-pad, depth / 2] };
  for (const [k, [x, z]] of Object.entries(at)) {
    const s = letterSprite(k, size);
    s.position.set(x, height + size / 2, z);
    group.add(s);
  }
  scene.add(group);

  const rose = document.createElement('div');
  rose.className = 'compass-rose';
  rose.innerHTML = `<svg viewBox="-50 -50 100 100" width="96" height="96">
    <circle r="46" fill="rgba(32,33,35,.82)" stroke="rgba(255,255,255,.25)"/>
    <g data-dial>
      <path d="M0,-36 L7,0 L0,6 L-7,0 Z" fill="#e2412f"/><path d="M0,36 L7,0 L0,-6 L-7,0 Z" fill="#cfcfcf"/>
      <text y="-37" fill="#fff" font-size="15" font-weight="700" text-anchor="middle" dominant-baseline="middle">N</text>
      <text x="38" fill="#ddd" font-size="12" text-anchor="middle" dominant-baseline="middle">E</text>
      <text y="38" fill="#ddd" font-size="12" text-anchor="middle" dominant-baseline="middle">S</text>
      <text x="-38" fill="#ddd" font-size="12" text-anchor="middle" dominant-baseline="middle">W</text>
    </g></svg>`;
  container.append(rose);
  const dial = rose.querySelector('[data-dial]');
  const f = new THREE.Vector3();

  /** Turn the rose so its N points where north (-Z) is on screen. */
  function update(camera, target) {
    f.copy(target).sub(camera.position);
    const azimuth = Math.atan2(f.x, -f.z);          // 0 looking north, +pi/2 looking east
    dial.setAttribute('transform', `rotate(${(-azimuth * 180) / Math.PI})`);
  }

  function setVisible(v) { group.visible = v; rose.hidden = !v; }

  return { group, update, setVisible };
}
