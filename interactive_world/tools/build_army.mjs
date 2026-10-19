// Builds assets/army.glb: one squad of 13 chibi soldiers (sword and shield, spearmen, a standard bearer) on a parade ground, with the
// clips Idle / March / Attack / Cheer. The squad faces +z and is centred on the origin; the viewer places it in front of the castle.
//
//   node tools/build_army.mjs
import { mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mulberry32 } from './lib/builder.mjs';
import { SceneBuilder } from './lib/glb_scene.mjs';
import * as S from './army/soldier.mjs';
import { clipChannels, CLIPS } from './army/motion.mjs';

const out = resolve(dirname(fileURLToPath(import.meta.url)), '../assets/army.glb');
mkdirSync(dirname(out), { recursive: true });

const rng = mulberry32(4242);
const scene = new SceneBuilder({ generator: 'interactive_world/tools/build_army.mjs' });
const qx = (deg) => [Math.sin((deg * Math.PI) / 360), 0, 0, Math.cos((deg * Math.PI) / 360)];
const qy = (rad) => [0, Math.sin(rad / 2), 0, Math.cos(rad / 2)];

// shared meshes: one per part, reused by every soldier
const mesh = {
  leg: scene.mesh('leg', S.leg()),
  torso: scene.mesh('torso', S.torso()),
  headSword: scene.mesh('head_sword', S.head('b')),
  headSpear: scene.mesh('head_spear', S.head('a')),
  headBanner: scene.mesh('head_banner', S.head('gold')),
  armSword: scene.mesh('arm_sword', S.arm('sword')),
  armSpear: scene.mesh('arm_spear', S.arm('spear')),
  armShield: scene.mesh('arm_shield', S.arm('shield')),
  armPlain: scene.mesh('arm_plain', S.arm('none')),
};
const flag = S.banner();
const poleMesh = scene.mesh('banner_pole', flag.pole);
const clothMesh = scene.mesh('banner_cloth', flag.flag);

// formation: [kind, x, z]  (front row has the largest z; the bearer stands in the middle of the back)
const XS = [-0.345, -0.115, 0.115, 0.345];
const ROWS = [
  [0.27, ['sword', 'sword', 'sword', 'sword']],
  [0.0, ['spear', 'sword', 'sword', 'spear']],
  [-0.27, ['sword', 'spear', 'spear', 'sword']],
];
const plan = [];
for (const [z, kinds] of ROWS) kinds.forEach((kind, i) => plan.push([kind, XS[i], z]));
plan.push(['banner', 0, -0.54]);

const units = [];
const rootChildren = [];
plan.forEach(([kind, x, z], i) => {
  const jitter = () => (rng() - 0.5);
  const sc = 0.96 + rng() * 0.09;
  const rest = { sword: -50, spear: -90, banner: -60, none: -30 };
  const nodes = {};
  nodes.legR = scene.node(`legR_${i}`, { mesh: mesh.leg, t: [S.J.legX, S.J.hip, 0] });
  nodes.legL = scene.node(`legL_${i}`, { mesh: mesh.leg, t: [-S.J.legX, S.J.hip, 0] });
  nodes.torso = scene.node(`torso_${i}`, { mesh: mesh.torso, t: [0, S.J.hip, 0] });
  nodes.head = scene.node(`head_${i}`, { mesh: kind === 'sword' ? mesh.headSword : kind === 'spear' ? mesh.headSpear : mesh.headBanner, t: [0, S.J.neck, 0] });
  nodes.armR = scene.node(`armR_${i}`, { mesh: { sword: mesh.armSword, spear: mesh.armSpear, banner: mesh.armPlain }[kind], t: [S.J.shoulderX, S.J.shoulder, 0], r: qx(rest[kind]) });
  nodes.armL = scene.node(`armL_${i}`, { mesh: kind === 'sword' ? mesh.armShield : mesh.armPlain, t: [-S.J.shoulderX, S.J.shoulder, 0], r: qx(-30) });
  const bodyChildren = [nodes.legR, nodes.legL, nodes.torso, nodes.head, nodes.armR, nodes.armL];
  if (kind === 'banner') {
    bodyChildren.push(scene.node(`pole_${i}`, { mesh: poleMesh }));
    nodes.banner = scene.node(`banner_${i}`, { mesh: clothMesh, t: flag.pivot });
    bodyChildren.push(nodes.banner);
  }
  nodes.body = scene.node(`body_${i}`, { t: [0, 0, 0], children: bodyChildren });
  const root = scene.node(`soldier_${i}`, { t: [x + jitter() * 0.03, 0, z + jitter() * 0.03], r: qy(jitter() * 0.14), s: [sc, sc, sc], children: [nodes.body] });
  rootChildren.push(root);
  units.push({ kind, phase: rng(), nodes });
});

const base = scene.node('parade_ground', { mesh: scene.mesh('parade_ground', S.squadBase(0, -0.13, 0.64, 0.72)) });
scene.root(scene.node('squad', { children: [base, ...rootChildren] }));
for (const name of CLIPS) scene.clip(name, clipChannels(name, units));

const bytes = scene.write(out, { source: 'procedural', units: units.length, clips: CLIPS });
console.log(`wrote ${out}: ${(bytes / 1e3).toFixed(0)} KB, ${units.length} soldiers, clips: ${CLIPS.join(', ')}`);
