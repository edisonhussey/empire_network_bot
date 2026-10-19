// Builds assets/castle.glb: the castle of keep_lv3.png, as a stylised low-poly game asset.
//
//   node tools/build_castle.mjs            writes assets/castle.glb
//
// Units follow the sheet: the footprint is 2.5 (x) by 2.0 (z); +z is the front, +x the right. Heights (sheet, elevation views):
// foundation 0.45, parapet 0.75, eaves 1.05, hall ridge 1.55, keep 1.8, tower 2.0, flag pole 2.7.
// The model is centred and sat on y = 0 by the viewer; here the footprint is simply built from the origin.
import { mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Prim, mulberry32, hex } from './lib/builder.mjs';
import { writeGlb } from './lib/glb.mjs';
import { buildStone } from './parts/stone.mjs';
import { buildRoofs } from './parts/roofs.mjs';
import { buildTowers } from './parts/towers.mjs';
import { buildDetails } from './parts/details.mjs';
import { buildGround } from './parts/ground.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const out = resolve(here, '../assets/castle.glb');
mkdirSync(dirname(out), { recursive: true });

export const PALETTE = {
  sand: hex('#a39478'), tan: hex('#7f7260'), brown: hex('#615548'), dark: hex('#3d3731'),
  blue1: hex('#1f5cb8'), blue2: hex('#17479a'), blue3: hex('#103573'), blue4: hex('#0a2552'), skyBlue: hex('#5aa0d2'),
  red: hex('#c4213d'), green: hex('#6f9c3a'), moss: hex('#58852f'), wood: hex('#4a3a2c'), woodLight: hex('#7a6246'),
  gold: hex('#d9a93a'), doorBlue: hex('#2b6fa8'), window: hex('#2a1713'), rock: hex('#8d8d90'),
};

// One primitive per material: they differ only in how they respond to light.
const prims = {
  stone: new Prim('stone', { roughness: 0.92 }),
  roof: new Prim('roof_tiles', { roughness: 0.38, metallic: 0.05 }),
  wood: new Prim('wood', { roughness: 0.85 }),
  grass: new Prim('grass', { roughness: 1 }),
  glow: new Prim('windows', { roughness: 0.6, emissive: [0.16, 0.035, 0.03] }),
  teamA: new Prim('team_a_ds', { roughness: 0.7, doubleSided: true }),   // flag, primary band (tinted per team at runtime)
  teamB: new Prim('team_b_ds', { roughness: 0.7, doubleSided: true }),   // flag, secondary band
  gold: new Prim('gold', { roughness: 0.35, metallic: 0.6 }),
  rock: new Prim('rock', { roughness: 0.95 }),
};

const ctx = { prims, P: PALETTE, rng: mulberry32(20261018) };
buildGround(ctx);
buildStone(ctx);
buildRoofs(ctx);
buildTowers(ctx);
buildDetails(ctx);

const info = writeGlb(out, Object.values(prims), { name: 'castle', extras: { source: 'keep_lv3.png', units: 'sheet units (footprint 2.5 x 2.0)', pivot: [1.25, 1.02] } });
const tris = Object.values(prims).reduce((n, p) => n + p.triangles, 0);
console.log(`wrote ${out}: ${(info.bytes / 1e6).toFixed(2)} MB, ${tris.toLocaleString()} triangles, ${info.primitives} materials`);
for (const p of Object.values(prims)) console.log(`  ${p.name.padEnd(12)} ${p.triangles.toLocaleString()} tris`);
