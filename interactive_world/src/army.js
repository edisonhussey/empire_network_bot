// The animated squad: loads assets/army.glb, plays its clips with an AnimationMixer, and can walk the squad towards the castle.
//
//   const army = loadArmy({ scene, GLTFLoader, THREE, decorate, outlineMat, onReady })
//   army.play('March'); army.walking = true; army.assault();
import * as THREE from 'three';

// The squad stands east of the castle facing west (-x) towards the tower, so the default 3/4 camera sees faces, not backs.
export const HOME = new THREE.Vector3(3.9, 0, 0.1);
const FAR = 6.4, NEAR = 2.55, SPEED = 0.2;           // walking range along x and ground speed (matches the march stride)

export function loadArmy({ scene, GLTFLoader, decorate, outlineMat, onReady }) {
  const root = new THREE.Group();
  root.position.copy(HOME);
  root.rotation.y = -Math.PI / 2;
  root.visible = false;
  scene.add(root);

  const api = {
    root, clips: [], current: null, walking: false, paused: false, mixer: null, sequence: null,
    show(v) { root.visible = v; },
    play(name, fade = 0.25) {
      const action = api.actions?.[name];
      if (!action || api.current === name) return;
      const prev = api.actions[api.current];
      action.reset().setEffectiveWeight(1).play();
      if (prev) prev.crossFadeTo(action, fade, false);
      api.current = name;
    },
    /** March in, strike twice, cheer, then go back and do it again. */
    assault() {
      root.visible = true;
      root.position.x = FAR;
      api.walking = true;
      api.sequence = { phase: 'march', t: 0 };
      api.play('March');
    },
    stopSequence() { api.sequence = null; },
    update(dt) {
      if (!api.mixer || api.paused) return;
      api.mixer.update(dt);
      const seq = api.sequence;
      if (api.walking && (api.current === 'March')) {
        root.position.x -= SPEED * dt;
        if (!seq && root.position.x < NEAR) root.position.x = FAR;
      }
      if (seq) {
        seq.t += dt;
        if (seq.phase === 'march' && root.position.x <= NEAR) { seq.phase = 'attack'; seq.t = 0; api.play('Attack', 0.2); }
        else if (seq.phase === 'attack' && seq.t > 2.6) { seq.phase = 'cheer'; seq.t = 0; api.play('Cheer', 0.2); }
        else if (seq.phase === 'cheer' && seq.t > 2.2) { seq.phase = 'march'; seq.t = 0; root.position.x = FAR; api.play('March', 0.3); }
      }
    },
  };

  new GLTFLoader().load('assets/army.glb', (gltf) => {
    root.add(gltf.scene);
    decorate(gltf.scene, outlineMat);
    api.mixer = new THREE.AnimationMixer(gltf.scene);
    api.actions = {};
    for (const clip of gltf.animations) {
      api.actions[clip.name] = api.mixer.clipAction(clip);
      api.clips.push(clip.name);
    }
    onReady?.(api);
  }, undefined, (e) => console.warn('army.glb failed to load', e));
  return api;
}
