// Builds assets/tower.glb: the robber baron watchtower, centred on its footprint (origin = centre, y = 0 ground, door towards +z).
//   node tools/build_tower.mjs [segments=2]
import { mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mulberry32 } from './lib/builder.mjs';
import { writeGlb } from './lib/glb.mjs';
import { makePrims, useTowerStone } from './mapkit/palette.mjs';
import { buildTower } from './tower/tower.mjs';

const out = resolve(dirname(fileURLToPath(import.meta.url)), '../assets/tower.glb');
mkdirSync(dirname(out), { recursive: true });
useTowerStone();
const segments = +process.argv[2] || 2;
const p = makePrims();
const { top } = buildTower(p, mulberry32(777), { segments });
const prims = Object.values(p);
const info = writeGlb(out, prims, { name: 'tower', extras: { source: 'robber_baron_castle.png', footprint: 3, segments, height: top } });
console.log(`wrote ${out}: ${(info.bytes / 1e3).toFixed(0)} KB, ${prims.reduce((n, q) => n + q.triangles, 0).toLocaleString()} triangles, height ${top.toFixed(2)}`);
