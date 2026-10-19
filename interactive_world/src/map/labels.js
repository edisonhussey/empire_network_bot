// Name tags: a small rounded label sprite above an entity.
import * as THREE from 'three';

export function nameTag(text, { width = 40 } = {}) {
  const c = document.createElement('canvas');
  c.width = 512; c.height = 96;
  const g = c.getContext('2d');
  g.font = '600 46px -apple-system, "Segoe UI", sans-serif';
  const w = Math.min(500, g.measureText(text).width + 44);
  g.fillStyle = 'rgba(28,24,20,.78)';
  g.beginPath(); g.roundRect((512 - w) / 2, 10, w, 76, 22); g.fill();
  g.fillStyle = '#fff'; g.textAlign = 'center'; g.textBaseline = 'middle';
  g.fillText(text, 256, 49);
  const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map: new THREE.CanvasTexture(c), depthTest: false, transparent: true, fog: false }));
  sprite.scale.set(width, width * (96 / 512), 1);
  sprite.renderOrder = 20;
  return sprite;
}
