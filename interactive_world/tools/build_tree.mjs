// Strips the showcase scene out of a downloaded tree model and writes a runtime-ready, metre-scaled asset: assets/pine.glb.
//   node tools/build_tree.mjs [source.glb] [--height 18] [--map-width 1344] [--keep-apex]
// The source (assets/tree.glb, a Blender export) is a product shot: the pine itself plus its own ground cover (16 grass tufts, 4 pebbles) and gold
// glitter, and, as presentation, a ground plane, "Eval" copies of the scene and four light-ray cards. This keeps the pine with its tufts, pebbles and
// glitter (nodes A1_Pine*, A1_GrassTuft*, A1_Pebble*, FX_GoldGlitter; not the A1_PineApex leaf rosette unless --keep-apex) and drops the presentation: ground, light rays, the second scene copy, the needles'
// self-lighting (emissive map) and their 3.7 MB normal map. Node transforms are baked, each material becomes one mesh, the needle map is shrunk, and
// the whole thing is scaled so the tree is `--height` metres tall (1 scene unit = 1 m) with its trunk base on y = 0 at x = z = 0.
// Needs `sips` (macOS) only for shrinking the needle map.
//
// Output nodes: pine_needles (alpha-masked), pine_bark | pine_stone, pine_grass, pine_glitter. extras: { height_m, radius_m, body: [...], extras: [...],
// tiers: [{ y0, y1, r }] } (needle tiers, from which the runtime builds its cheap far-distance version).
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { BinPack, writeContainer } from './lib/gltf_io.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const flag = (name, dflt) => { const i = args.indexOf(`--${name}`); return i < 0 ? dflt : args.splice(i, 2)[1]; };
const KEEP_APEX = args.includes('--keep-apex') && args.splice(args.indexOf('--keep-apex'), 1);   // the leaf rosette on top: its dark leaves read as a black spike from the strategy camera
const HEIGHT = +flag('height', 18);
const MAP_W = +flag('map-width', 1344);               // needle map is 2688x2048 in the source; this width keeps the aspect ratio
const src = resolve(args[0] ?? resolve(here, '../assets/tree.glb'));
const out = resolve(here, '../assets/pine.glb');

const KEEP = new RegExp(`^(A1_Pine${KEEP_APEX ? '' : '(?!Apex)'}|A1_GrassTuft|A1_Pebble|FX_GoldGlitter)`);   // everything that belongs to the pine object
const BODY = /^A1_Pine/;                                           // the tree itself: sets the height and the ground level
const TIER = /^A1_Pine(Tier|Apex)/;                                // needle pieces (for the far version)
// source material -> output mesh; `body` meshes are the tree, the rest are its ground cover and sparkle (drawn on the nearest trees only)
const GROUPS = [
  { name: 'pine_needles', material: /PineFeather/, body: true, shrink: true, normals: false },
  { name: 'pine_bark', material: /Bark/, body: true },
  { name: 'pine_stone', material: /Stone/ },
  { name: 'pine_grass', material: /GrassTuft/ },
  { name: 'pine_glitter', material: /GoldGlitter/ },
];

// ---- read the source container ------------------------------------------------------------------------------------------------------------------
const buf = readFileSync(src);
const jsonLen = buf.readUInt32LE(12);
const gltf = JSON.parse(buf.subarray(20, 20 + jsonLen).toString('utf8'));
const bin = buf.subarray(20 + jsonLen + 8);
const COMP = { 5120: [Int8Array, 1], 5121: [Uint8Array, 1], 5122: [Int16Array, 2], 5123: [Uint16Array, 2], 5125: [Uint32Array, 4], 5126: [Float32Array, 4] };
const WIDTH = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4 };
function read(index) {
  const a = gltf.accessors[index], v = gltf.bufferViews[a.bufferView];
  const [Type, size] = COMP[a.componentType], w = WIDTH[a.type];
  const stride = v.byteStride ?? size * w, start = (v.byteOffset ?? 0) + (a.byteOffset ?? 0);
  const outArr = new Type(a.count * w);
  const dv = new DataView(bin.buffer, bin.byteOffset);
  const get = { 5126: 'getFloat32', 5123: 'getUint16', 5125: 'getUint32', 5121: 'getUint8', 5122: 'getInt16', 5120: 'getInt8' }[a.componentType];
  for (let i = 0; i < a.count; i++) for (let c = 0; c < w; c++) outArr[i * w + c] = dv[get](start + i * stride + c * size, true);
  return outArr;
}
const viewBytes = (i) => bin.subarray(gltf.bufferViews[i].byteOffset ?? 0, (gltf.bufferViews[i].byteOffset ?? 0) + gltf.bufferViews[i].byteLength);

// ---- 4x4 matrices (column-major, as glTF) -------------------------------------------------------------------------------------------------------
const mul = (a, b) => { const o = new Array(16).fill(0); for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) for (let k = 0; k < 4; k++) o[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k]; return o; };
function local(n) {
  if (n.matrix) return n.matrix;
  const [qx, qy, qz, qw] = n.rotation ?? [0, 0, 0, 1], [sx, sy, sz] = n.scale ?? [1, 1, 1], [tx, ty, tz] = n.translation ?? [0, 0, 0];
  return [
    (1 - 2 * (qy * qy + qz * qz)) * sx, 2 * (qx * qy + qz * qw) * sx, 2 * (qx * qz - qy * qw) * sx, 0,
    2 * (qx * qy - qz * qw) * sy, (1 - 2 * (qx * qx + qz * qz)) * sy, 2 * (qy * qz + qx * qw) * sy, 0,
    2 * (qx * qz + qy * qw) * sz, 2 * (qy * qz - qx * qw) * sz, (1 - 2 * (qx * qx + qy * qy)) * sz, 0,
    tx, ty, tz, 1,
  ];
}

// ---- collect the pine: matching nodes of the scene that has the most of them (the second one is the full tree; the first lacks a tier) -----------
const sceneOf = (s) => { let n = 0; const walk = (i) => { if (KEEP.test(gltf.nodes[i].name)) n++; (gltf.nodes[i].children ?? []).forEach(walk); }; s.nodes.forEach(walk); return n; };
const scene = gltf.scenes.reduce((best, s) => (sceneOf(s) > sceneOf(best) ? s : best));
const pieces = [];   // { name, material, pos, nor, uv, idx } with node transforms baked in
const walk = (i, parent) => {
  const n = gltf.nodes[i], m = mul(parent, local(n));
  if (KEEP.test(n.name) && n.mesh !== undefined) {
    for (const p of gltf.meshes[n.mesh].primitives) {
      const pos = read(p.attributes.POSITION), nor = read(p.attributes.NORMAL);
      const uv = p.attributes.TEXCOORD_0 !== undefined ? read(p.attributes.TEXCOORD_0) : new Float32Array((pos.length / 3) * 2);
      const idx = p.indices !== undefined ? read(p.indices) : Uint32Array.from({ length: pos.length / 3 }, (_, k) => k);
      for (let k = 0; k < pos.length; k += 3) {
        const [x, y, z] = [pos[k], pos[k + 1], pos[k + 2]];
        pos[k] = m[0] * x + m[4] * y + m[8] * z + m[12]; pos[k + 1] = m[1] * x + m[5] * y + m[9] * z + m[13]; pos[k + 2] = m[2] * x + m[6] * y + m[10] * z + m[14];
        const [nx, ny, nz] = [nor[k], nor[k + 1], nor[k + 2]];   // uniform scale only in this model, so the plain rotation part is enough
        const tx = m[0] * nx + m[4] * ny + m[8] * nz, ty = m[1] * nx + m[5] * ny + m[9] * nz, tz = m[2] * nx + m[6] * ny + m[10] * nz, l = Math.hypot(tx, ty, tz) || 1;
        nor[k] = tx / l; nor[k + 1] = ty / l; nor[k + 2] = tz / l;
      }
      pieces.push({ name: n.name, material: gltf.materials[p.material], pos, nor, uv, idx });
    }
  }
  (n.children ?? []).forEach((c) => walk(c, m));
};
scene.nodes.forEach((i) => walk(i, [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]));
if (!pieces.some((p) => BODY.test(p.name))) throw new Error(`no A1_Pine* meshes in ${src}`);

// The apex's authored normals are scrambled (it shades black under the map's lighting); give it up-and-outward normals like the rest of the crown.
for (const p of pieces) {
  if (!/Apex/.test(p.name)) continue;
  for (let k = 0; k < p.pos.length; k += 3) {
    const x = p.pos[k], z = p.pos[k + 2], l = Math.hypot(x, 1.5, z);
    p.nor[k] = x / l; p.nor[k + 1] = 1.5 / l; p.nor[k + 2] = z / l;
  }
}

// ---- normalise: base on y = 0, scaled to HEIGHT metres (the tree sets the scale; its cover and sparkles follow) ------------------------------------
const bounds = (list) => { const b = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] }; for (const p of list) for (let k = 0; k < p.pos.length; k++) { const c = k % 3; b.min[c] = Math.min(b.min[c], p.pos[k]); b.max[c] = Math.max(b.max[c], p.pos[k]); } return b; };
const body = bounds(pieces.filter((p) => BODY.test(p.name)));
const s = HEIGHT / (body.max[1] - body.min[1]);
for (const p of pieces) for (let k = 0; k < p.pos.length; k += 3) { p.pos[k] *= s; p.pos[k + 1] = (p.pos[k + 1] - body.min[1]) * s; p.pos[k + 2] *= s; }
const tiers = pieces.filter((p) => TIER.test(p.name)).map((p) => { const b = bounds([p]); return { y0: b.min[1], y1: b.max[1], r: Math.max(b.max[0] - b.min[0], b.max[2] - b.min[2]) / 2 }; }).sort((a, b) => a.y0 - b.y0);
const fin = bounds(pieces.filter((p) => BODY.test(p.name)));
const radius = Math.max(...[0, 2].flatMap((c) => [-fin.min[c], fin.max[c]]));
const round = (v) => Math.round(v * 1000) / 1000;

// ---- images: copied as they are, except the needle map which is shrunk ------------------------------------------------------------------------------
const tmp = mkdtempSync(join(tmpdir(), 'pine-'));
function image(index, shrink) {
  const bytes = Buffer.from(viewBytes(gltf.images[index].bufferView));
  if (!shrink) return bytes;
  const f = join(tmp, `${index}.png`);
  writeFileSync(f, bytes);
  try { execFileSync('sips', ['--resampleWidth', String(MAP_W), f, '--out', f], { stdio: 'pipe' }); return readFileSync(f); }
  catch (e) { console.warn(`sips failed (${e.message.split('\n')[0]}): keeping the full-size needle map`); return bytes; }
}

// ---- write the runtime asset ------------------------------------------------------------------------------------------------------------------------
const pack = new BinPack();
const json = { asset: { version: '2.0', generator: 'interactive_world/tools/build_tree.mjs' }, scene: 0, scenes: [{ nodes: [] }], nodes: [], meshes: [], materials: [], images: [], textures: [], samplers: [{ magFilter: 9729, minFilter: 9987 }] };
const texture = (textureIndex, shrink = false) => {
  const view = pack.push(image(gltf.textures[textureIndex].source, shrink));
  json.images.push({ bufferView: view, mimeType: 'image/png' });
  json.textures.push({ sampler: 0, source: json.images.length - 1 });
  return { index: json.textures.length - 1 };
};
const usesEmissiveStrength = [];
for (const g of GROUPS) {
  const group = pieces.filter((p) => g.material.test(p.material.name));
  if (!group.length) { console.warn(`no pieces for ${g.name}`); continue; }
  const sm = group[0].material, pbr = sm.pbrMetallicRoughness ?? {};
  const vertexCount = group.reduce((n, p) => n + p.pos.length / 3, 0), indexCount = group.reduce((n, p) => n + p.idx.length, 0);
  const pos = new Float32Array(vertexCount * 3), nor = new Float32Array(vertexCount * 3), uv = new Float32Array(vertexCount * 2), idx = new Uint32Array(indexCount);
  let v = 0, ii = 0;
  for (const p of group) {
    pos.set(p.pos, v * 3); nor.set(p.nor, v * 3); uv.set(p.uv, v * 2);
    for (let k = 0; k < p.idx.length; k++) idx[ii++] = p.idx[k] + v;
    v += p.pos.length / 3;
  }
  const b = bounds([{ pos }]);
  const attributes = {
    POSITION: pack.accessor(pos, 5126, 'VEC3', vertexCount, { target: 34962, min: b.min, max: b.max }),
    NORMAL: pack.accessor(nor, 5126, 'VEC3', vertexCount, { target: 34962 }),
    ...(pbr.baseColorTexture ? { TEXCOORD_0: pack.accessor(uv, 5126, 'VEC2', vertexCount, { target: 34962 }) } : {}),
  };
  const indices = pack.accessor(idx, 5125, 'SCALAR', indexCount, { target: 34963 });
  const strength = sm.extensions?.KHR_materials_emissive_strength?.emissiveStrength;
  if (strength) usesEmissiveStrength.push(g.name);
  json.materials.push({
    name: g.name,
    pbrMetallicRoughness: {
      ...(pbr.baseColorTexture ? { baseColorTexture: texture(pbr.baseColorTexture.index, g.shrink) } : {}),
      ...(pbr.baseColorFactor ? { baseColorFactor: pbr.baseColorFactor } : {}),
      metallicFactor: pbr.metallicFactor ?? 0, roughnessFactor: pbr.roughnessFactor ?? 0.85,
    },
    // the needles' normal map is dropped: at map distances it adds nothing and it costs 3.7 MB; the needles' emissive map (a few gold sparkles) is dropped too
    ...(sm.normalTexture && g.normals !== false ? { normalTexture: { ...texture(sm.normalTexture.index), scale: sm.normalTexture.scale ?? 1 } } : {}),
    ...(g.name === 'pine_glitter' && sm.emissiveFactor ? { emissiveFactor: sm.emissiveFactor, extensions: { KHR_materials_emissive_strength: { emissiveStrength: strength ?? 1 } } } : {}),
    ...(sm.alphaMode === 'MASK' ? { alphaMode: 'MASK', alphaCutoff: sm.alphaCutoff ?? 0.5 } : {}),
    doubleSided: true,
  });
  json.meshes.push({ name: g.name, primitives: [{ attributes, indices, material: json.materials.length - 1, mode: 4 }] });
  json.nodes.push({ name: g.name, mesh: json.meshes.length - 1 });
  json.scenes[0].nodes.push(json.nodes.length - 1);
  console.log(`${g.name.padEnd(13)} ${String(vertexCount).padStart(6)} vertices, ${String(indexCount / 3).padStart(6)} triangles`);
}
if (usesEmissiveStrength.length) json.extensionsUsed = ['KHR_materials_emissive_strength'];
json.extras = {
  height_m: round(fin.max[1]), radius_m: round(radius),
  body: GROUPS.filter((g) => g.body).map((g) => g.name), extras: GROUPS.filter((g) => !g.body).map((g) => g.name),
  tiers: tiers.map((t) => ({ y0: round(t.y0), y1: round(t.y1), r: round(t.r) })),
};
mkdirSync(dirname(out), { recursive: true });
const bytes = writeContainer(out, json, pack);
rmSync(tmp, { recursive: true, force: true });
console.log(`wrote ${out}: ${(bytes / 1e6).toFixed(2)} MB, ${json.extras.height_m} m tall, crown radius ${json.extras.radius_m} m, ${tiers.length} needle tiers`);
