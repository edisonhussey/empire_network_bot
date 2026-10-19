// Shared glTF binary plumbing: a buffer packer, Prim -> glTF primitive conversion, and the .glb container. No dependencies.
// Used by glb.mjs (single static mesh: the castle) and glb_scene.mjs (node tree + animation clips: the army).
import { writeFileSync } from 'node:fs';
import { srgbToLinear } from './builder.mjs';

const pad4 = (n) => (n + 3) & ~3;

export class BinPack {
  constructor() {
    this.chunks = [];
    this.offset = 0;
    this.views = [];
    this.accessors = [];
  }

  /** Append a typed array as a buffer view; returns the view index. */
  push(typed, target) {
    const bytes = Buffer.from(typed.buffer, typed.byteOffset, typed.byteLength);
    const start = this.offset;
    this.chunks.push(bytes);
    this.offset += bytes.length;
    const padding = pad4(this.offset) - this.offset;
    if (padding) {
      this.chunks.push(Buffer.alloc(padding));
      this.offset += padding;
    }
    const view = { buffer: 0, byteOffset: start, byteLength: bytes.length };
    if (target) view.target = target;
    this.views.push(view);
    return this.views.length - 1;
  }

  /** Append a typed array and describe it with an accessor; returns the accessor index. */
  accessor(typed, componentType, type, count, { target, ...extra } = {}) {
    const bufferView = this.push(typed, target);
    this.accessors.push({ bufferView, componentType, count, type, ...extra });
    return this.accessors.length - 1;
  }
}

/** Convert a Prim (flat-shaded triangle soup, sRGB vertex colours) into a glTF primitive; pushes its material into `materials`. */
export function addPrimitive(pack, prim, materials, materialIndex = null) {
  const n = prim.pos.length / 3;
  const pos = new Float32Array(prim.pos);
  const nor = new Float32Array(prim.nor);
  const col = new Uint16Array(n * 4);
  for (let i = 0; i < n; i++) {
    for (let c = 0; c < 3; c++) col[i * 4 + c] = Math.round(Math.min(1, Math.max(0, srgbToLinear(prim.col[i * 3 + c]))) * 65535);
    col[i * 4 + 3] = 65535;
  }
  const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < n; i++) for (let c = 0; c < 3; c++) {
    min[c] = Math.min(min[c], pos[i * 3 + c]);
    max[c] = Math.max(max[c], pos[i * 3 + c]);
  }
  const aPos = pack.accessor(pos, 5126, 'VEC3', n, { target: 34962, min, max });
  const aNor = pack.accessor(nor, 5126, 'VEC3', n, { target: 34962 });
  const aCol = pack.accessor(col, 5123, 'VEC4', n, { target: 34962, normalized: true });
  if (materialIndex === null) {
    const m = prim.material;
    materials.push({
      name: prim.name,
      pbrMetallicRoughness: { baseColorFactor: [1, 1, 1, 1], metallicFactor: m.metallic ?? 0, roughnessFactor: m.roughness ?? 0.85 },
      ...(m.emissive ? { emissiveFactor: m.emissive } : {}),
      ...(m.doubleSided ? { doubleSided: true } : {}),
    });
    materialIndex = materials.length - 1;
  }
  return { attributes: { POSITION: aPos, NORMAL: aNor, COLOR_0: aCol }, material: materialIndex, mode: 4 };
}

/** Write the .glb container (json chunk + binary chunk). Returns the file size. */
export function writeContainer(path, json, pack) {
  json.accessors = pack.accessors;
  json.bufferViews = pack.views;
  json.buffers = [{ byteLength: pack.offset }];
  let jsonBytes = Buffer.from(JSON.stringify(json), 'utf8');
  jsonBytes = Buffer.concat([jsonBytes, Buffer.alloc(pad4(jsonBytes.length) - jsonBytes.length, 0x20)]);
  const bin = Buffer.concat(pack.chunks);
  const total = 12 + 8 + jsonBytes.length + 8 + bin.length;
  const header = Buffer.alloc(12);
  header.writeUInt32LE(0x46546c67, 0); // 'glTF'
  header.writeUInt32LE(2, 4);
  header.writeUInt32LE(total, 8);
  const chunkHead = (length, type) => {
    const b = Buffer.alloc(8);
    b.writeUInt32LE(length, 0);
    b.writeUInt32LE(type, 4);
    return b;
  };
  writeFileSync(path, Buffer.concat([header, chunkHead(jsonBytes.length, 0x4e4f534a), jsonBytes, chunkHead(bin.length, 0x004e4942), bin]));
  return total;
}
