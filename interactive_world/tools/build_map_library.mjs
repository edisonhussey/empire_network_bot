// Builds assets/map_library.glb: every catalogue piece as its own named node, laid out in a showcase grid for preview. The map reads the nodes by
// name (positions are ignored): each piece is centred on its own origin, y = 0 on the ground, so it can be placed, rotated and scaled freely.
//   node tools/build_map_library.mjs
import { mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mulberry32 } from './lib/builder.mjs';
import { SceneBuilder } from './lib/glb_scene.mjs';
import { makePrims } from './mapkit/palette.mjs';
import { CATALOG } from './mapkit/catalog.mjs';

const out = resolve(dirname(fileURLToPath(import.meta.url)), '../assets/map_library.glb');
mkdirSync(dirname(out), { recursive: true });
const scene = new SceneBuilder({ generator: 'interactive_world/tools/build_map_library.mjs' });
const groups = {};
const extras = { assets: {} };
let total = 0;
for (const a of CATALOG) {
  const p = makePrims();
  a.build(p, mulberry32(a.seed));
  const prims = Object.values(p);
  const tris = prims.reduce((n, q) => n + q.triangles, 0);
  total += tris;
  (groups[a.group] ??= []).push({ a, mesh: scene.mesh(a.name, prims), tris });
  extras.assets[a.name] = { footprint: a.fp, outline: a.outline, group: a.group, triangles: tris };
}
// showcase layout: one row per group
const nodes = [];
let row = 0;
for (const [group, items] of Object.entries(groups)) {
  items.forEach(({ a, mesh }, i) => nodes.push(scene.node(a.name, { mesh, t: [i * 4.2, 0, row * 5] })));
  row++;
}
scene.root(scene.node('map_library', { children: nodes }));
const bytes = scene.write(out, extras);
// one small preview scene per group (assets/preview/<group>.glb) so the pieces can be inspected in the model viewer
mkdirSync(resolve(dirname(out), 'preview'), { recursive: true });
for (const [group, items] of Object.entries(groups)) {
  const pv = new SceneBuilder({ generator: 'preview' });
  const ns = items.map(({ a }, i) => {
    const p = makePrims();
    a.build(p, mulberry32(a.seed));
    return pv.node(a.name, { mesh: pv.mesh(a.name, Object.values(p)), t: [(i - (items.length - 1) / 2) * (group === 'trees' ? 2.0 : 4.4), 0, 0] });
  });
  pv.root(pv.node(group, { children: ns }));
  pv.write(resolve(dirname(out), `preview/${group}.glb`));
}
console.log(`wrote ${out}: ${(bytes / 1e6).toFixed(2)} MB, ${CATALOG.length} pieces, ${total.toLocaleString()} triangles`);
for (const a of CATALOG) console.log(`  ${a.name.padEnd(14)} ${String(extras.assets[a.name].triangles).padStart(6)} tris`);
